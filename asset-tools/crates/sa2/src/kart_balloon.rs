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
