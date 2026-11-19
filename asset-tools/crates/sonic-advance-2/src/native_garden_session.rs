//! Standalone cartridge laboratory. High-level commands enter named callbacks;
//! they are not a port of Garden cursor/Chao AI, audio or sprite task scheduling.
use crate::{
    Error, VerifiedRom, native_garden_memory as memory, native_garden_progress as progress,
    native_garden_second as bounce, native_growth as growth, native_hatching as hatch,
    native_shop_state as shop, native_weeds as weeds, shop::TinyShopCatalog,
};
use serde_json::{Value, json};
use std::num::NonZeroU16;

fn fail(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
enum Scene {
    Garden,
    Memory(Box<memory::NativeMemoryRound>),
    Bounce(Box<bounce::BouncePractice>),
}
pub struct GardenSession<'a, 'r> {
    rom: &'a VerifiedRom<'r>,
    scene: Scene,
    saved: progress::GardenSaveSession,
    shop: shop::NativeShopState,
    prices: shop::NativeShopPrices,
    fruit: TinyShopCatalog,
    hatch: hatch::NativeHatchSource,
    plants: weeds::NativeWeeds,
    soil: weeds::NativeWeedAssets,
    frame: u32,
    held: u16,
    last_result: Option<(&'static str, bool, u16)>,
}
impl<'a, 'r> GardenSession<'a, 'r> {
    pub fn load(rom: &'a VerifiedRom<'r>, flash: &[u8], seed: u32) -> Result<Self, Error> {
        let p = rom.garden_bytes();
        let config = u32::from_le_bytes(rom.range("garden", 8, 4)?.try_into().unwrap());
        let saved = progress::GardenSaveSession::load(
            flash,
            progress::GardenLoadServices::from_seed(config, seed),
        )?;
        let shop = shop::NativeShopState::restore(
            shop::NativeShopFields::read_raw_fields(saved.record_bytes()).map_err(fail)?,
            saved.rings,
        )
        .map_err(fail)?;
        let plants = weeds::NativeWeeds::restore(
            weeds::NativeWeedFields::read_raw_fields(saved.record_bytes()).map_err(fail)?,
        );
        Ok(Self {
            rom,
            saved,
            shop,
            plants,
            prices: shop::NativeShopPrices::from_verified_payload(p).map_err(fail)?,
            fruit: TinyShopCatalog::from_verified_payload(p).map_err(fail)?,
            hatch: hatch::NativeHatchSource::from_verified_payload(p).map_err(fail)?,
            soil: weeds::NativeWeedAssets::from_verified_payload(p).map_err(fail)?,
            scene: Scene::Garden,
            frame: 0,
            held: 0,
            last_result: None,
        })
    }
    pub fn at_garden(&self) -> bool {
        matches!(self.scene, Scene::Garden)
    }
    pub fn picture(&self) -> Result<crate::graphics::RenderedMap, Error> {
        match &self.scene {
            Scene::Garden => {
                let mut image = crate::native_garden_background::render(self.rom, false)?;
                let mut current = *self.saved.record_bytes();
                self.shop
                    .fields
                    .write_raw_fields(&mut current)
                    .map_err(fail)?;
                self.plants
                    .fields
                    .write_raw_fields(&mut current)
                    .map_err(fail)?;
                crate::native_garden_home_art::paint(self.rom, &current, &mut image)?;
                Ok(image)
            }
            Scene::Memory(r) => crate::native_garden_art::memory(self.rom, &r.view()),
            Scene::Bounce(r) => crate::native_garden_art::bounce(self.rom, &r.view()),
        }
    }
    fn at_home(&self) -> Result<(), Error> {
        if !matches!(self.scene, Scene::Garden) {
            return Err(fail("Return to the Garden first"));
        }
        Ok(())
    }
    pub fn start(&mut self, game: &str) -> Result<(), Error> {
        self.at_home()?;
        if self.shop.held.is_some() || self.plants.pull.is_some() {
            return Err(fail("Finish holding the item or pulling the weed first"));
        }
        self.scene = match game {
            "memory" => Scene::Memory(Box::new(
                memory::A2neMemorySource::from_verified_payload(self.rom.garden_bytes())
                    .map_err(fail)?
                    .begin_round(self.saved.rng_seed)
                    .map_err(fail)?,
            )),
            "bounce" => Scene::Bounce(Box::new(
                bounce::BouncePractice::new(self.rom).map_err(fail)?,
            )),
            _ => return Err(fail("A2NE has Memory and Bounce")),
        };
        self.shop.close_stock();
        self.held = 0;
        Ok(())
    }
    /// One accepted physical input per video frame. No network or wall clock.
    pub fn tick(&mut self, held: u16) -> Result<(), Error> {
        if held > 255 {
            return Err(fail("Unsupported GBA button word"));
        }
        let finished = match &mut self.scene {
            Scene::Garden => {
                if held != 0 {
                    return Err(fail("Use named Garden actions; pad drives the minigames"));
                }
                let mut rng = growth::NativeRng::new(self.saved.rng_seed);
                let budget = NonZeroU16::new(1024).unwrap();
                let rollback = (self.plants.fields.spawn_ticks == 1).then(|| self.plants.clone());
                let update = self
                    .plants
                    .spawn_tick(0, &self.soil, &mut rng, budget)
                    .map_err(fail)?;
                if update.event == weeds::SpawnEvent::Pending {
                    if let Some(previous) = rollback {
                        self.plants = previous;
                    }
                    return Err(fail("Weed spawn search exceeded the per-frame bound"));
                }
                self.saved.rng_seed = rng.state();
                if self.plants.pull.is_some() {
                    self.plants.update_pull(0, &self.soil).map_err(fail)?;
                }
                for i in 0..weeds::WEED_CAPACITY {
                    self.plants.update_weed(i as u8, 0).map_err(fail)?;
                }
                self.shop.advance_shop_ticks(1, 0);
                None
            }
            Scene::Memory(r) => {
                r.step(memory::NativeButtons {
                    pressed: held & !self.held,
                    held,
                })
                .map_err(fail)?;
                self.saved.rng_seed = r.rng_seed();
                r.finished().map(|v| {
                    (
                        "memory",
                        v.won,
                        u16::from(v.rings),
                        progress::credit_signed_byte(self.saved.rings, v.rings),
                    )
                })
            }
            Scene::Bounce(r) => {
                r.tick(held).map_err(fail)?;
                r.finished().map(|(won, rings)| {
                    (
                        "bounce",
                        won,
                        rings,
                        progress::credit_unsigned_halfword(self.saved.rings, rings),
                    )
                })
            }
        };
        self.frame = self.frame.wrapping_add(1);
        self.held = held;
        if let Some((game, won, reward, balance)) = finished {
            self.last_result = Some((game, won, reward));
            self.set_wallet(balance);
            self.scene = Scene::Garden;
            self.held = 0;
        }
        Ok(())
    }
    fn set_wallet(&mut self, balance: u32) {
        self.saved.rings = balance;
        self.shop.rings = balance;
        // 0200296a: the native ring-display task refreshes the transfer mirror.
        self.saved.record_mut()[0x64..0x68].copy_from_slice(&balance.to_be_bytes());
    }
    /// Practice enters the unobstructed world-drop branch after a cursor pick.
    /// x/y are the item's ground anchor (the cursor is nineteen pixels above).
    pub fn place(&mut self, x: u8, y: u8) -> Result<(), Error> {
        self.at_home()?;
        if x == 0 || x >= 176 || y < 19 {
            return Err(fail("Drop point outside the Garden"));
        }
        let h = self.shop.held.ok_or_else(|| fail("No held item"))?;
        let y = y.min(159);
        let terrain =
            self.rom.range("garden", 0x21f34, 440)?[usize::from(y / 8) * 22 + usize::from(x / 8)];
        let permitted = if h.item == shop::NativeShopItem::Toy(1) {
            terrain != 0
        } else {
            terrain == 0
        };
        if !permitted {
            return Err(fail("Native terrain prevents this drop"));
        }
        let p = shop::NativePosition { x, y };
        match h.item {
            shop::NativeShopItem::Fruit(_) => {
                self.shop.fields.fruit_positions[usize::from(
                    h.fruit_slot
                        .ok_or_else(|| fail("Missing held fruit slot"))?,
                )] = p
            }
            shop::NativeShopItem::Egg(_) => self.shop.fields.egg_position = p,
            shop::NativeShopItem::Toy(id) => self.shop.fields.toy_positions[usize::from(id)] = p,
        }
        self.shop.held = None;
        self.shop.close_stock();
        Ok(())
    }
    pub fn hatch(&mut self) -> Result<(), Error> {
        self.at_home()?;
        if self
            .shop
            .held
            .is_some_and(|h| !matches!(h.item, shop::NativeShopItem::Egg(_)))
        {
            return Err(fail("Place the held item first"));
        }
        let mut rng = growth::NativeRng::new(self.saved.rng_seed);
        self.shop
            .fields
            .write_raw_fields(self.saved.record_mut())
            .map_err(fail)?;
        hatch::hatch_into_empty_native_slot(
            self.saved.record_mut(),
            &self.hatch,
            hatch::NativeHatchContext {
                alternate_name_table: false,
                frame_counter: self.frame,
            },
            &mut rng,
        )
        .map_err(fail)?;
        self.saved.rng_seed = rng.state();
        self.shop = shop::NativeShopState::restore(
            shop::NativeShopFields::read_raw_fields(self.saved.record_bytes()).map_err(fail)?,
            self.saved.rings,
        )
        .map_err(fail)?;
        Ok(())
    }
    pub fn stock(&mut self) -> Result<Value, Error> {
        self.at_home()?;
        let mut rng = growth::NativeRng::new(self.saved.rng_seed);
        let s = self
            .shop
            .open_stock(self.frame, 0, &mut rng)
            .map_err(fail)?;
        self.saved.rng_seed = rng.state();
        Ok(
            json!({"fruit":s.fruit_available,"egg":s.egg,"toy":s.toy,"fruitPrices":self.prices.fruit,"eggPrices":self.prices.egg,"toyPrices":self.prices.toy}),
        )
    }
    pub fn buy(&mut self, item: shop::NativeShopItem) -> Result<(), Error> {
        self.at_home()?;
        let y = item.row_y().ok_or_else(|| fail("Unknown shop row"))?;
        self.shop
            .purchase(
                &self.prices,
                item,
                shop::NativePurchaseInput {
                    cursor_x: 24,
                    cursor_y: y - 8,
                    pressed_keys: 1,
                    held_keys: 1,
                    ..Default::default()
                },
            )
            .map_err(fail)?;
        self.set_wallet(self.shop.rings);
        Ok(())
    }
    /// Callback entry shortcut: the caller selects an already present fruit.
    /// In 0x020053FA the fruit actor gives the Chao its ID and clears that
    /// saved slot to FF. Case 13 (0x0200493C) applies its signed growth row.
    pub fn feed(&mut self, slot: usize) -> Result<(), Error> {
        self.at_home()?;
        if self.shop.fields.chao_field_58 == 255 {
            return Err(fail("Hatch a Chao first"));
        }
        let id = *self
            .shop
            .fields
            .fruit_ids
            .get(slot)
            .ok_or_else(|| fail("Invalid fruit slot"))?;
        let fruit = self
            .fruit
            .fruits
            .get(usize::from(id))
            .ok_or_else(|| fail("No fruit in that slot"))?;
        let mut g =
            growth::NativeGrowth::read_raw_fields(self.saved.record_bytes()).map_err(fail)?;
        let mut rng = growth::NativeRng::new(self.saved.rng_seed);
        growth::apply_fruit_checked(&mut g, fruit.raw, &mut rng).map_err(fail)?;
        g.write_raw_fields(self.saved.record_mut()).map_err(fail)?;
        self.saved.rng_seed = rng.state();
        self.shop.fields.fruit_ids[slot] = 255;
        if self
            .shop
            .held
            .is_some_and(|h| h.fruit_slot == Some(slot as u8))
        {
            self.shop.held = None;
        }
        self.shop.close_stock();
        Ok(())
    }
    pub fn pull(&mut self, slot: u8) -> Result<(), Error> {
        self.at_home()?;
        let w = self
            .plants
            .weeds
            .get(usize::from(slot))
            .ok_or_else(|| fail("Invalid weed slot"))?;
        let input = weeds::NativePullInput {
            pressed_keys: 1,
            held_keys: 1,
            cursor_x: i16::from(w.x) + 8 + i16::from(self.soil.hitbox[0]),
            cursor_y: w.y - 8 + i16::from(self.soil.hitbox[2]),
            shop_boundary: -1,
            cursor_holding: self.shop.held.is_some(),
            ..Default::default()
        };
        self.plants
            .begin_pull(slot, input, &self.soil)
            .map_err(fail)?;
        Ok(())
    }
    /// Prepare the native journal in a copy. A refused or failed external write
    /// leaves the live generation, physical sector and flash image unchanged.
    pub fn export_flash(
        &mut self,
        write: impl FnOnce(&[u8]) -> std::io::Result<()>,
    ) -> Result<(), Error> {
        self.at_home()?;
        let mut staged = self.saved.clone();
        self.shop
            .fields
            .write_raw_fields(staged.record_mut())
            .map_err(fail)?;
        self.plants
            .fields
            .write_raw_fields(staged.record_mut())
            .map_err(fail)?;
        if !staged.write_memory(0)?.succeeded {
            return Err(fail("Memory flash programming failed"));
        }
        write(staged.flash_bytes()).map_err(fail)?;
        self.saved = staged;
        Ok(())
    }
    pub fn save(&mut self) -> Result<&[u8], Error> {
        self.export_flash(|_| Ok(()))?;
        Ok(self.saved.flash_bytes())
    }
    pub fn view(&self) -> Value {
        match &self.scene {
            Scene::Garden => {
                let growth = growth::NativeGrowth::read_raw_fields(self.saved.record_bytes())
                    .expect("fixed length record");
                json!({"scene":"garden","frame":self.frame,"wallet":self.saved.rings,"generation":self.saved.version,
                    "chao":self.shop.fields.chao_field_58!=255,"egg":(self.shop.fields.egg_id!=255).then_some(self.shop.fields.egg_id),
                    "fruitSlots":self.shop.fields.fruit_ids.map(|id|(id!=255).then_some(id)),
                    "growth":(self.shop.fields.chao_field_58!=255).then(||json!({"mood":growth.mood,"belly":growth.belly,
                        "skills":growth.skills.map(|s|[u32::from(s.level),u32::from(s.experience),u32::from(s.points)])})),
                    "weedSlots":self.plants.weeds.iter().enumerate().filter(|(_,w)|w.present).map(|(i,w)|json!([i,w.x,w.y,!w.emerging])).collect::<Vec<_>>(),
                    "lastResult":self.last_result})
            }
            Scene::Memory(r) => {
                let v = r.view();
                json!({"scene":"memory","phase":format!("{:?}",v.phase),"cursor":v.cursor,"pairs":v.matched_pairs,"mistakes":v.mistakes,"rings":v.rings,
                "board":v.cells.chunks(6).map(|row|row.iter().map(|cell|if !cell.occupied{'.'}else{cell.face.map(|x|char::from(b'A'+x)).unwrap_or('?')}).collect::<String>()).collect::<Vec<_>>()})
            }
            Scene::Bounce(r) => json!({"scene":"bounce","state":r.view()}),
        }
    }
}
