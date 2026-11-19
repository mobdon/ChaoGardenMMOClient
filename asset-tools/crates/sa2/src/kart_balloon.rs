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
    VehicleCallback560c,
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
        4 => return Ok(Award::VehicleCallback560c),
        _ => {
            let sample = random();
            if sample >= 32768 {
                return Err("expected 15-bit native random sample");
            }
            // Native f32 multiplies by 2^-15 and 4 are exact on this domain.
            let index = usize::from(sample) / 8192;
            if index == 3 {
                return Ok(Award::VehicleCallback560c);
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
            assert_eq!(
                award(0, 0, 1, 0, 0, || sample),
                Ok(Award::VehicleCallback560c)
            );
        }
    }
}
