use serde_json::Value;
use sonic_advance_2_assets::{
    VerifiedRom, gba, native_garden_progress::GardenLoadServices,
    native_garden_session::GardenSession, native_save, native_shop_state::NativeShopItem,
    shop::TinyShopCatalog,
};

const MEMORY_TRACE_SHA256: &str =
    "2d579d408af0b18e73681ae48400060300b13f01826084a89f14106780dd8a42";
fn rom_bytes() -> Vec<u8> {
    std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap()
}
fn selected(flash: &[u8]) -> Vec<u8> {
    native_save::inspect_flash(flash)
        .unwrap()
        .selected
        .unwrap()
        .raw_bytes()
        .to_vec()
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn wallet(view: &Value) -> u64 {
    view["wallet"].as_u64().unwrap()
}
fn named_scene(view: &Value, name: &str) {
    assert_eq!(view["scene"], name);
}

#[test]
#[ignore = "A2NE_ROM and A2NE_MEMORY_SESSION_INPUTS required; original-Thumb default-seed winning inputs"]
fn memory_inputs_earn_wallet_then_hatch_buy_feed_save_and_reopen() {
    let source = rom_bytes();
    let original_hash = gba::sha256(&source);
    let rom = VerifiedRom::parse(&source).unwrap();
    let input_flash = vec![255; 65536];
    let mut session = GardenSession::load(&rom, &input_flash, 0).unwrap();
    assert_eq!(GardenLoadServices::from_seed(0, 0).rng_seed_before, 0x3039);
    assert_eq!(wallet(&session.view()), 0);
    assert_eq!(session.view()["chao"], false);
    assert_eq!(session.view()["egg"], 0);
    let baseline = session.save().unwrap().to_vec();
    let baseline_record = selected(&baseline);
    assert_eq!(u32_at(&baseline_record, 0x18), 0x3039);
    let trace = std::fs::read(
        std::env::var_os("A2NE_MEMORY_SESSION_INPUTS").expect("A2NE_MEMORY_SESSION_INPUTS"),
    )
    .unwrap();
    assert_eq!(gba::sha256(&trace), MEMORY_TRACE_SHA256);
    let seed = u32_at(&trace, 0);
    let frames = u32_at(&trace, 4) as usize;
    assert_eq!((seed, frames), (0x3039, 1775));
    assert_eq!(trace.len(), 8 + frames * 16 + 12);
    session.start("memory").unwrap();
    let mut previous = 0u16;
    for (frame, row) in trace[8..8 + frames * 16]
        .as_chunks::<16>()
        .0
        .iter()
        .enumerate()
    {
        let held = u16::from(row[0]);
        assert_eq!(
            held & previous,
            0,
            "physical input has release edges at{frame}"
        );
        previous = held;
        session.tick(held).unwrap();
        let view = session.view();
        if row[1] == 0 {
            named_scene(&view, "garden");
        } else {
            named_scene(&view, "memory");
            let phase = match row[1] {
                4 => "Reveal",
                5 => "Shuffle",
                6 => "Matching",
                7..=9 => "Result",
                other => panic!("Unexpected native Memory phase{other}"),
            };
            assert_eq!(view["phase"], phase, "phase{frame}");
            if phase == "Matching" {
                assert_eq!(
                    view["cursor"],
                    serde_json::json!([row[3], row[4]]),
                    "cursor{frame}"
                );
            }
            assert_eq!(view["mistakes"], row[5], "mistakes{frame}");
            assert_eq!(view["pairs"], row[6], "pairs{frame}");
            assert_eq!(view["rings"], row[7], "rings{frame}");
        }
    }
    assert_eq!(u32_at(&trace, 8 + frames * 16 + 8), 90);
    let view = session.view();
    named_scene(&view, "garden");
    assert_eq!(view["lastResult"], serde_json::json!(["memory", true, 90]));
    assert_eq!(wallet(&view), 90);
    let earned_flash = session.save().unwrap().to_vec();
    let earned_record = selected(&earned_flash);
    assert_eq!(
        u32_at(&earned_record, 0x18),
        u32_at(&trace, 8 + (frames - 1) * 16 + 12)
    );
    assert_eq!(
        &earned_record[0x6a..],
        &baseline_record[0x6a..],
        "minigame cannot overwrite opaque Chao bytes"
    );
    session.hatch().unwrap();
    let hatched = session.view();
    assert_eq!(hatched["chao"], true);
    assert!(hatched["egg"].is_null());
    assert_eq!(wallet(&hatched), 90);
    assert!(session.hatch().is_err());
    let stock = session.stock().unwrap();
    assert_eq!(stock["fruit"][0], true);
    assert_eq!(stock["fruitPrices"][0], 30);
    session.buy(NativeShopItem::Fruit(0)).unwrap();
    assert_eq!(wallet(&session.view()), 60);
    assert_eq!(session.view()["fruitSlots"][0], 0);
    assert!(session.buy(NativeShopItem::Fruit(0)).is_err());
    assert!(session.start("memory").is_err());
    assert!(session.place(0, 80).is_err());
    let terrain = rom.range("garden", 0x21f34, 440).unwrap();
    let points: Vec<_> = (19u8..160)
        .flat_map(|y| (1u8..176).map(move |x| (x, y)))
        .collect();
    let blocked = points
        .iter()
        .copied()
        .find(|&(x, y)| terrain[usize::from(y / 8) * 22 + usize::from(x / 8)] != 0)
        .unwrap();
    assert!(session.place(blocked.0, blocked.1).is_err());
    let ground = points
        .iter()
        .copied()
        .find(|&(x, y)| terrain[usize::from(y / 8) * 22 + usize::from(x / 8)] == 0)
        .unwrap();
    session.place(ground.0, ground.1).unwrap();
    assert!(session.place(ground.0, ground.1).is_err());
    let before_feed_flash = session.save().unwrap().to_vec();
    let before_feed = selected(&before_feed_flash);
    assert_eq!(before_feed[0x58], 0);
    assert_eq!(before_feed[0x59], 255);
    assert_eq!(before_feed[0x5a], 0);
    assert_eq!(&before_feed[0x21..0x23], &[ground.0, ground.1]);
    assert_eq!(&before_feed[0x73..0x75], &[100, 100]);
    session.feed(0).unwrap();
    assert!(session.feed(0).is_err());
    let fed = session.view();
    assert!(fed["fruitSlots"][0].is_null());
    assert_eq!(wallet(&fed), 60);
    let final_flash = session.save().unwrap().to_vec();
    let final_record = selected(&final_flash);
    let fruit = TinyShopCatalog::from_verified_payload(rom.garden_bytes())
        .unwrap()
        .fruits[0];
    assert_eq!(
        final_record[0x73],
        (100 + i16::from(fruit.raw[0]) * 20).clamp(0, 200) as u8
    );
    assert_eq!(
        final_record[0x74],
        (100 + i16::from(fruit.raw[1]) * 20).clamp(0, 200) as u8
    );
    for index in 0..5 {
        assert_eq!(
            final_record[0x78 + index],
            (i16::from(fruit.raw[index + 2]) * 10).max(0) as u8
        );
        assert_eq!(final_record[0x88 + index], 1);
    }
    for at in 0..final_record.len() {
        let allowed = (0..8).contains(&at)
            || (0x18..0x1c).contains(&at)
            || at == 0x5a
            || (0x73..0x75).contains(&at)
            || (0x78..0x7d).contains(&at);
        if !allowed {
            assert_eq!(
                final_record[at], before_feed[at],
                "feed altered unrelated byte{at:#x}"
            );
        }
    }
    assert_eq!(u32_at(&final_record, 0xc), 60);
    assert_eq!(
        u32_at(&final_record, 0x64).swap_bytes(),
        60,
        "native bank mirror"
    );
    assert_eq!(
        &final_flash[..10 * 4096],
        &input_flash[..10 * 4096],
        "campaign sectors untouched"
    );
    let mut reopened = GardenSession::load(&rom, &final_flash, 12345).unwrap();
    assert_eq!(wallet(&reopened.view()), 60);
    assert_eq!(reopened.view()["growth"], fed["growth"]);
    assert_eq!(reopened.view()["fruitSlots"], fed["fruitSlots"]);
    assert_eq!(reopened.view()["chao"], true);
    let reopened_flash = reopened.save().unwrap().to_vec();
    let reopened_record = selected(&reopened_flash);
    for at in 0..final_record.len() {
        if at >= 8 && at != 0x68 {
            assert_eq!(
                reopened_record[at], final_record[at],
                "reopen changed record{at:#x}"
            );
        }
    }
    assert_eq!(input_flash, vec![255; 65536]);
    assert_eq!(gba::sha256(&source), original_hash);
}

#[test]
#[ignore = "A2NE_ROM required; physical Bounce A-every60 trace reaches original return with earned7rings"]
fn bounce_inputs_earn_seven_and_reopen_without_duplicate_award() {
    let source = rom_bytes();
    let rom = VerifiedRom::parse(&source).unwrap();
    let input_flash = vec![255; 65536];
    let mut session = GardenSession::load(&rom, &input_flash, 0).unwrap();
    assert_eq!(wallet(&session.view()), 0);
    session.start("bounce").unwrap();
    assert!(session.save().is_err());
    assert!(session.start("memory").is_err());
    for frame in 0..1097 {
        session.tick(u16::from(frame % 60 == 0)).unwrap();
        if frame < 1096 {
            named_scene(&session.view(), "bounce");
        }
    }
    let ended = session.view();
    named_scene(&ended, "garden");
    assert_eq!(ended["lastResult"], serde_json::json!(["bounce", false, 7]));
    assert_eq!(wallet(&ended), 7);
    for _ in 0..120 {
        session.tick(0).unwrap();
    }
    assert_eq!(wallet(&session.view()), 7);
    session.stock().unwrap();
    assert!(session.buy(NativeShopItem::Fruit(0)).is_err());
    assert_eq!(wallet(&session.view()), 7);
    let saved = session.save().unwrap().to_vec();
    let record = selected(&saved);
    assert_eq!(u32_at(&record, 0xc), 7);
    assert_eq!(u32_at(&record, 0x64).swap_bytes(), 7);
    let reopened = GardenSession::load(&rom, &saved, 999).unwrap();
    assert_eq!(wallet(&reopened.view()), 7);
    assert_eq!(reopened.view()["chao"], false);
    assert_eq!(&saved[..40960], &input_flash[..40960]);
    assert_eq!(input_flash, vec![255; 65536]);
}

#[test]
#[ignore = "A2NE_ROM required; native timed weed spawn/pull through session, no injected reward or weed"]
fn default_garden_spawns_then_pulls_weed_without_wallet_or_opaque_mutation() {
    let source = rom_bytes();
    let rom = VerifiedRom::parse(&source).unwrap();
    let input_flash = vec![255; 65536];
    let mut session = GardenSession::load(&rom, &input_flash, 0).unwrap();
    let initial = session.save().unwrap().to_vec();
    let initial_record = selected(&initial);
    let mut mature = None;
    for _ in 0..6000 {
        session.tick(0).unwrap();
        let view = session.view();
        let slots = view["weedSlots"].as_array().unwrap();
        if let Some(weed) = slots.iter().find(|w| w[3].as_bool() == Some(true)) {
            mature = Some(weed[0].as_u64().unwrap() as u8);
            break;
        }
    }
    let slot = mature.expect("native initialized timer must yield a mature weed");
    session.pull(slot).unwrap();
    assert!(session.start("memory").is_err());
    for _ in 0..200 {
        session.tick(0).unwrap();
    }
    assert!(
        !session.view()["weedSlots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w[0].as_u64() == Some(u64::from(slot)))
    );
    assert_eq!(wallet(&session.view()), 0);
    let saved = session.save().unwrap().to_vec();
    let record = selected(&saved);
    assert_eq!(&record[0x6a..], &initial_record[0x6a..]);
    let reopened = GardenSession::load(&rom, &saved, 91).unwrap();
    assert_eq!(wallet(&reopened.view()), 0);
    assert!(reopened.view()["weedSlots"].as_array().unwrap().is_empty());
    assert_eq!(reopened.view()["chao"], false);
    assert_eq!(&saved[..40960], &input_flash[..40960]);
    assert_eq!(input_flash, vec![255; 65536]);
}
