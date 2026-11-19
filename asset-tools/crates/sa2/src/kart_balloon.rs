//! CartD text +0xa2d4..0xa48c, after the collision partner matched a kart.
//! The caller owns task state, collision lookup and imported callbacks.
#[derive(Debug, PartialEq, Eq)]
pub enum Award {
    None,
    Charge {
        next: i32,
        display: i16,
        notify: Option<i32>,
    },
    Invincibility,
}

/// Host boundary for the two imported calls. A failed allocation returns None.
pub trait InvincibilityHost {
    fn request_audio(&mut self, filename: &str);
    fn create_task(
        &mut self,
        flags: u32,
        priority: u32,
        update: u32,
        name: &str,
    ) -> Option<(&mut u8, &mut u32)>;
}

/// CartD +0x560c and +0x6c44. Task callback addresses are section-1 offsets.
/// Returned task fields correspond to work byte +1 and task word +0x14.
pub fn activate_invincibility(
    work_108: &mut i32,
    work_288: &mut i32,
    work_78: i32,
    host: &mut impl InvincibilityHost,
) {
    *work_108 = 1;
    *work_288 = 0x4ec;
    host.request_audio("invncibl.adx");
    if let Some((player, display)) = host.create_task(2, 3, 0x6d7c, "Muteki") {
        *player = work_78 as u8;
        *display = 0x6ca8;
    }
}
/// Returns the native award for one matched contact. A negative driver still
/// pops the balloon upstream. Random draws are requested only on the default arm.
/// The imported random function returns a 15-bit sample; reject other inputs.
pub fn award(
    kind: i32,
    driver: i32,
    work_44: i32,
    charge: i32,
    clock: u32,
    random: impl FnOnce() -> u16,
) -> Result<Award, &'static str> {
    const VALUES: [i32; 3] = [5, 10, 20]; // section 5 +0x2ed3c0
    if driver < 0 || work_44 == 0 {
        return Ok(Award::None);
    }
    let (amount, display, notify) = match kind {
        1..=3 => {
            let amount = VALUES[(kind - 1) as usize];
            (amount, VALUES[(clock % 3) as usize], Some(amount))
        }
        4 => return Ok(Award::Invincibility),
        _ => {
            let sample = random();
            if sample >= 32768 {
                return Err("expected 15-bit native random sample");
            }
            // Native f32 multiplies by 2^-15 and 4 are exact on this domain.
            let index = usize::from(sample) / 8192;
            if index == 3 {
                return Ok(Award::Invincibility);
            }
            (VALUES[index], VALUES[index], None)
        }
    };
    Ok(Award::Charge {
        next: charge.wrapping_add(amount),
        display: display as i16,
        notify,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_survives_missing_effect_task() {
        struct Host {
            allocated: bool,
            player: u8,
            display: u32,
            audio: bool,
        }
        impl InvincibilityHost for Host {
            fn request_audio(&mut self, filename: &str) {
                assert_eq!(filename, "invncibl.adx");
                self.audio = true;
            }
            fn create_task(
                &mut self,
                flags: u32,
                priority: u32,
                update: u32,
                name: &str,
            ) -> Option<(&mut u8, &mut u32)> {
                assert!(self.audio);
                assert_eq!((flags, priority, update, name), (2, 3, 0x6d7c, "Muteki"));
                self.allocated
                    .then_some((&mut self.player, &mut self.display))
            }
        }
        for allocated in [false, true] {
            let mut host = Host {
                allocated,
                player: 0,
                display: 0,
                audio: false,
            };
            let (mut flag, mut timer) = (0, 19);
            activate_invincibility(&mut flag, &mut timer, 0x123, &mut host);
            assert_eq!((flag, timer), (1, 1260));
            assert_eq!(
                (host.player, host.display),
                if allocated { (0x23, 0x6ca8) } else { (0, 0) }
            );
        }
    }
    #[test]
    fn fixed_amount_and_display_are_independent() {
        assert_eq!(
            award(1, 0, 1, i32::MAX, 2, || panic!()),
            Ok(Award::Charge {
                next: i32::MIN + 4,
                display: 20,
                notify: Some(5)
            })
        );
        assert_eq!(award(0, -1, 1, 0, 0, || panic!()), Ok(Award::None));
    }
    #[test]
    fn native_random_bucket_edges() {
        for (sample, amount) in [
            (0, 5),
            (8191, 5),
            (8192, 10),
            (16383, 10),
            (16384, 20),
            (24575, 20),
        ] {
            assert_eq!(
                award(0, 0, 1, 3, 0, || sample),
                Ok(Award::Charge {
                    next: 3 + amount,
                    display: amount as i16,
                    notify: None
                })
            );
        }
        for sample in [24576, 32767] {
            assert_eq!(award(0, 0, 1, 0, 0, || sample), Ok(Award::Invincibility));
        }
    }
}

/// Imported task allocation, random generator and task deletion. A host is bound
/// to the current Muteki parent, which is also the native allocation's parent.
pub trait MutekiHost {
    type Child;
    fn delete_parent(&mut self);
    fn allocate_child(&mut self, flags: u32, update: u32) -> Option<Self::Child>;
    fn random_sample(&mut self) -> i32;
    fn initialize_child(&mut self, child: Self::Child, angles: [i32; 2], player: u8, display: u32);
}

/// CartD +0x6d7c. work_108 belongs to the vehicle selected by signed player byte.
pub fn tick_muteki(timer: &mut u16, player: u8, work_108: i32, host: &mut impl MutekiHost) {
    if work_108 == 0 {
        host.delete_parent();
        return;
    }
    *timer = timer.wrapping_add(1);
    for _ in 0..1 + (*timer & 1) {
        if let Some(child) = host.allocate_child(2, 0x6ef0) {
            // Section 4 +0x2c0, +0x2bc. Native random samples are 15-bit.
            let mut angle = || ((host.random_sample() as f32 / 32768.0) * 65536.0) as i32;
            let angles = [angle(), angle()];
            host.initialize_child(child, angles, player, 0x6f2c);
        }
    }
}

/// CartD +0x6ef0. Returns true when the native routine requests deletion.
pub fn tick_muteki_child(age: &mut u8) -> bool {
    if *age >= 16 {
        true
    } else {
        *age += 1;
        false
    }
}

#[cfg(test)]
mod muteki_tests {
    use super::*;
    #[derive(Default)]
    struct Host {
        attempts: u32,
        draws: i32,
        deleted: bool,
        particles: Vec<([i32; 2], u8)>,
    }
    impl MutekiHost for Host {
        type Child = ();
        fn delete_parent(&mut self) {
            self.deleted = true;
        }
        fn allocate_child(&mut self, flags: u32, update: u32) -> Option<()> {
            assert_eq!((flags, update), (2, 0x6ef0));
            self.attempts += 1;
            (self.attempts != 1).then_some(())
        }
        fn random_sample(&mut self) -> i32 {
            self.draws += 1;
            32768 - self.draws
        }
        fn initialize_child(&mut self, _: (), angles: [i32; 2], player: u8, display: u32) {
            assert_eq!(display, 0x6f2c);
            self.particles.push((angles, player));
        }
    }
    #[test]
    fn wraps_then_alternates_attempts_and_only_draws_for_allocated_children() {
        let mut h = Host::default();

        let mut timer = u16::MAX;
        tick_muteki(&mut timer, 1, 1, &mut h);
        assert_eq!((timer, h.attempts, h.draws), (0, 1, 0));
        tick_muteki(&mut timer, 1, -1, &mut h);
        assert_eq!(h.particles, vec![([65534, 65532], 1), ([65530, 65528], 1)]);
        tick_muteki(&mut timer, 1, 0, &mut h);
        assert!(h.deleted);
        assert_eq!((timer, h.attempts, h.draws), (1, 3, 4));
    }
    #[test]
    fn age_sixteen_is_visible_before_next_update_requests_deletion() {
        let mut age = 0;
        for _ in 0..16 {
            assert!(!tick_muteki_child(&mut age));
        }
        assert!(tick_muteki_child(&mut age));
        assert_eq!(age, 16);
    }
}

/// Fields retained between calls to the native balloon task (+0xa138).
#[derive(Debug, Default)]
pub struct Balloon {
    pub mode: i8,
    pub selector: i32,
    pub phase: u8,
    pub timer: u16,
    pub scale: [f32; 3],
}

/// Vehicle task work fields accessed by CartD's matched-contact branch.
#[derive(Debug, Default)]
pub struct BalloonVehicle {
    pub work_44: i32,
    pub work_70: i32,
    pub work_78: i32,
    pub work_d8: i32,
    pub work_108: i32,
    pub work_288: i32,
}
pub trait RewardHost: InvincibilityHost {
    fn engine_8002f8c4(&mut self, args: [i32; 4]);
    fn engine_80063584(&mut self, driver: i32, value: i16);
    fn engine_8006b710(&mut self, args: [i32; 4]);
    fn random_sample(&mut self) -> u16;
}

impl BalloonVehicle {
    fn collect(&mut self, kind: i32, clock: u32, host: &mut impl RewardHost) -> crate::Result<()> {
        if self.work_70 < 0 {
            return Ok(());
        }
        if self.work_44 != 0 {
            host.engine_8002f8c4([self.work_78, 1, 0x30, 0]);
            match award(
                kind,
                self.work_70,
                self.work_44,
                self.work_d8,
                clock,
                || host.random_sample(),
            )
            .map_err(crate::Error::invalid)?
            {
                Award::Charge {
                    next,
                    display,
                    notify,
                } => {
                    host.engine_80063584(self.work_70, display);
                    self.work_d8 = next;
                    if let Some(n) = notify {
                        host.engine_80063584(self.work_70, n as i16);
                    }
                }
                Award::Invincibility => activate_invincibility(
                    &mut self.work_108,
                    &mut self.work_288,
                    self.work_78,
                    host,
                ),
                Award::None => (),
            }
        }
        host.engine_8006b710([0x800d, 0, 0, 0]);
        Ok(())
    }
}
/// Host observations and callbacks not owned by the balloon task.
/// Each contact entry is visited in original order. Rust resolves its task
/// through CartD +0x1714 and passes the matched slot to the host for award and sound calls.
pub trait BalloonHost {
    fn install_collision(&mut self);
    fn collision_flags(&self) -> u16;
    fn contact_count(&self) -> i16;
    fn contact_task(&self, index: i16) -> u32;
    fn vehicle_tasks(&self) -> [u32; 6];
    type Rewards: RewardHost;
    fn vehicle_and_rewards(&mut self, slot: usize) -> (&mut BalloonVehicle, &mut Self::Rewards);
    fn trig(&mut self, angle: i32) -> (f32, f32);
    fn register_collision(&mut self);
    fn cleanup(&mut self, has_entry: bool);
}

impl Balloon {
    /// Dispatch table: CartD section 5 +0x2ed3cc. The caller owns display slots.
    pub fn tick(
        &mut self,
        range: crate::kart_object_range::ObjectRange,
        clock: u32,
        has_entry: bool,
        host: &mut impl BalloonHost,
    ) -> crate::Result<()> {
        if range.outside()? && self.mode != 4 {
            return Ok(());
        }
        match self.mode {
            0..=2 => {
                if self.mode == 0 {
                    host.install_collision();
                }
                if self.mode != 2 {
                    self.scale = [0.0; 3];
                    self.mode = 2;
                }
                for v in &mut self.scale {
                    *v += f32::from_bits(0x3d088889);
                }
                if self.scale[0] >= 1.0 {
                    self.mode = 3;
                }
                host.register_collision();
            }
            3 => {
                if host.collision_flags() & 1 != 0 {
                    let mut i = 0;
                    while i < host.contact_count() {
                        if let Some(slot) =
                            crate::kart_registry::slot(&host.vehicle_tasks(), host.contact_task(i))
                        {
                            self.mode = 4;
                            self.timer = 0;
                            let (vehicle, rewards) = host.vehicle_and_rewards(slot);
                            vehicle.collect(self.selector, clock, rewards)?;
                        }
                        i += 1;
                    }
                }
                let angle = ((self.phase as f32 * 1.40625) * f32::from_bits(0x43360b61)) as i32;
                let (s, c) = host.trig(angle);
                self.scale = [1.0 + 0.02 * s, 1.0, 1.0 + 0.02 * c];
                self.phase = self.phase.wrapping_add(1);
                host.register_collision();
            }
            4 => {
                for v in &mut self.scale {
                    *v += 0.03;
                }
                self.timer = self.timer.wrapping_add(1);
                if self.timer > 60 {
                    self.mode = 5;
                    self.timer = 3600;
                }
            }
            5 => {
                self.timer = self.timer.wrapping_sub(1);
                if self.timer == 0 {
                    self.mode = 1;
                }
            }
            6 => host.cleanup(has_entry),
            _ => (),
        }
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[derive(Default)]
    struct Host {
        installs: usize,
        registrations: usize,
        hits: usize,
        vehicles: [BalloonVehicle; 6],
        rewards: Calls,
    }
    #[derive(Default)]
    struct Calls(Vec<Vec<i32>>);
    impl InvincibilityHost for Calls {
        fn request_audio(&mut self, _: &str) {
            panic!()
        }
        fn create_task(&mut self, _: u32, _: u32, _: u32, _: &str) -> Option<(&mut u8, &mut u32)> {
            panic!()
        }
    }
    impl RewardHost for Calls {
        fn engine_8002f8c4(&mut self, args: [i32; 4]) {
            self.0.push(args.to_vec());
        }
        fn engine_80063584(&mut self, driver: i32, value: i16) {
            self.0.push(vec![driver, value.into()]);
        }
        fn engine_8006b710(&mut self, args: [i32; 4]) {
            self.0.push(args.to_vec());
        }
        fn random_sample(&mut self) -> u16 {
            panic!()
        }
    }
    impl BalloonHost for Host {
        fn install_collision(&mut self) {
            self.installs += 1;
        }
        fn collision_flags(&self) -> u16 {
            1
        }
        fn contact_count(&self) -> i16 {
            3
        }
        fn contact_task(&self, i: i16) -> u32 {
            [88, 99, 42][i as usize]
        }
        fn vehicle_tasks(&self) -> [u32; 6] {
            [0, 42, 0, 0, 0, 88]
        }
        type Rewards = Calls;
        fn vehicle_and_rewards(&mut self, slot: usize) -> (&mut BalloonVehicle, &mut Calls) {
            assert_eq!(slot, [5, 1][self.hits]);
            self.hits += 1;
            (&mut self.vehicles[slot], &mut self.rewards)
        }
        fn trig(&mut self, angle: i32) -> (f32, f32) {
            assert_eq!(angle, 0);
            (0.0, 1.0)
        }
        fn register_collision(&mut self) {
            self.registrations += 1;
        }
        fn cleanup(&mut self, _: bool) {
            panic!()
        }
    }
    #[test]
    fn collect_out_of_range_pop_and_in_range_respawn() {
        let near = crate::kart_object_range::ObjectRange {
            first: 0.0,
            views: 1,
            second: None,
        };
        let far = crate::kart_object_range::ObjectRange {
            first: 4_000_000.0,
            ..near
        };
        let mut b = Balloon {
            selector: 3,
            ..Default::default()
        };
        let mut h = Host::default();
        h.vehicles[5].work_44 = 1;
        h.vehicles[5].work_78 = 4;
        for _ in 0..30 {
            b.tick(near, 1, true, &mut h).unwrap();
        }
        assert_eq!(b.mode, 3);
        b.tick(near, 1, true, &mut h).unwrap();
        assert_eq!(
            (b.mode, b.timer, b.scale, h.hits),
            (4, 0, [1.0, 1.0, 1.02], 2)
        );
        assert_eq!(h.vehicles[5].work_d8, 20);
        assert_eq!(h.vehicles[1].work_d8, 0);
        assert_eq!(
            h.rewards.0,
            vec![
                vec![4, 1, 48, 0],
                vec![0, 10],
                vec![0, 20],
                vec![0x800d, 0, 0, 0],
                vec![0x800d, 0, 0, 0]
            ]
        );
        for _ in 0..61 {
            b.tick(far, 1, true, &mut h).unwrap();
        }
        assert_eq!((b.mode, b.timer), (5, 3600));
        b.tick(far, 1, true, &mut h).unwrap();
        assert_eq!(b.timer, 3600);
        for _ in 0..3600 {
            b.tick(near, 1, true, &mut h).unwrap();
        }
        assert_eq!((b.mode, b.timer, h.registrations), (1, 0, 31));
        b.tick(near, 1, true, &mut h).unwrap();
        assert_eq!((b.mode, h.installs, h.registrations), (2, 1, 32));
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    mode: i8,
    timer: u16,
    position: [f32; 3],
    yaw: i32,
    scale: [f32; 3],
    current_view_metric: f32,
}

/// CartD +0xa650. Keep the draw multiplier separate from the original DDS alpha.
pub fn display(bytes: &[u8], pak: &[u8], s: DisplayInput) -> crate::Result<serde_json::Value> {
    use serde_json::json;
    crate::kart_progress::verify(bytes)?;
    let rel = crate::city_rel::CityRel::read_identified(bytes, 80)?;
    let data = rel.data(5)?;
    let constants = rel.data(4)?;
    let f = |at: usize| f32::from_be_bytes(constants[at..at + 4].try_into().unwrap());
    if s.current_view_metric >= f(0x1c0) || s.mode == 5 {
        return Ok(json!({"draw":null}));
    }
    let invalid = || crate::Error::invalid("unresolved balloon display resource");
    let target = |site| {
        rel.reference(1, site)
            .filter(|r| (r.module, r.section, r.kind) == (80, 5, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let ptr = |at| {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let list = target(0xa74a)?;
    let object = target(0xa772)?;
    let attach = ptr(object + 4)?;
    if data[list as usize + 4..list as usize + 8] != 1u32.to_be_bytes() {
        return Err(invalid());
    }
    let name = std::ffi::CStr::from_bytes_until_nul(&data[ptr(ptr(list)?)? as usize..])
        .map_err(|_| invalid())?
        .to_str()
        .map_err(|_| invalid())?;
    let member = format!("objtex_common\\{name}.dds");
    let bank = crate::archive::read_pak(pak)?;
    let mut matches = bank.iter().filter(|e| e.name.eq_ignore_ascii_case(&member));
    let entry = matches.next().ok_or_else(invalid)?;
    if matches.next().is_some() {
        return Err(invalid());
    }
    let image = crate::texture::decode_dds(entry.data)?;
    let geometry =
        crate::kart_story_target::vertices(&rel, attach, &mut crate::visual::Material::default())?;
    let word = |at: u32| u32::from_be_bytes(data[at as usize..at as usize + 4].try_into().unwrap());
    // The native draw traverses a Ninja object. This pinned object has one node.
    if rel.local_pointer(5, object + 44)?.is_some() || rel.local_pointer(5, object + 48)?.is_some()
    {
        return Err(invalid());
    }
    let vec3 = |at| [word(at), word(at + 4), word(at + 8)];
    let node = json!({"flags":word(object),"position":(vec3(object+8).map(f32::from_bits)),
        "angles":(vec3(object+20).map(|v|v as i32)),"scale":(vec3(object+32).map(f32::from_bits))});
    let popping = s.mode == 4;
    let alpha = if popping {
        (60 - i32::from(s.timer)) as f32 / f(0x3a8)
    } else {
        1.0
    };
    Ok(
        json!({"draw":{"object":object,"attach":attach,"textureList":list,"geometry":geometry,
        "position":s.position,"yaw":s.yaw,"scale":s.scale,"materialMultiplier":[alpha,1.0,1.0,1.0],
        "controlMasks":if popping {[255,2048]} else {[-2049,0]},
        "node":node,"texture":{"offset":entry.offset,"length":entry.length,"entry":entry.name,"width":image.width,"height":image.height,
        "ddsSha256":crate::manifest::sha256(entry.data),"rgbaSha256":crate::manifest::sha256(&image.rgba)}}}),
    )
}
