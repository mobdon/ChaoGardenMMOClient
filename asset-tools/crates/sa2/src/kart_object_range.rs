//! CartD +0x1854: visibility gate shared by story balloons and kart rings.
use crate::{Error, Result};

/// Metrics are the original vector helper returns, before comparison with the
/// section-4 +0x1c0 constant. The second import is only called in two-view mode
/// after view zero fails, so callers need not supply an unused observation.
#[derive(Clone, Copy)]
pub struct ObjectRange {
    pub first: f32,
    pub views: i32,
    pub second: Option<f32>,
}

impl ObjectRange {
    pub fn outside(self) -> Result<bool> {
        const LIMIT: f32 = 4_000_000.0;
        if self.first < LIMIT {
            return Ok(false);
        }
        if self.views != 2 {
            return Ok(true);
        }
        let metric = self
            .second
            .ok_or_else(|| Error::invalid("missing second view range metric"))?;
        Ok(!matches!(
            metric.partial_cmp(&LIMIT),
            Some(std::cmp::Ordering::Less)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_and_second_view_short_circuit() {
        let gate = |first, views, second| {
            ObjectRange {
                first,
                views,
                second,
            }
            .outside()
        };
        assert!(!gate(3_999_999.75, 2, None).unwrap());
        assert!(gate(4_000_000.0, 2, None).is_err());
        assert!(gate(4_000_000.0, 1, Some(0.0)).unwrap());
        assert!(!gate(4_000_000.0, 2, Some(3_999_999.75)).unwrap());
        assert!(gate(f32::NAN, 2, Some(f32::NAN)).unwrap());
    }
}
