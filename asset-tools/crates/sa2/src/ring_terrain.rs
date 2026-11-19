//! Terrain branch of GSNE8P Ring. External query/allocation results are consumed only on call.
use crate::{
    stage_gameplay::{RingMode, StageRing},
    Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

const NO_SHADOW: f32 = -1_000_000.0;
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Observations {
    timer: Option<u8>,
    reference_position: Option<[f32; 3]>,
    shadow: Option<Shadow>,
    allocated: Option<bool>,
    random: Option<f32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Shadow {
    height: f32,
    yaw_after: i32,
}

#[derive(Default)]
pub(crate) struct Terrain {
    timer: u8,
    height: f32,
}
impl Terrain {
    pub fn tick(
        &mut self,
        mode: RingMode,
        ring: &mut StageRing,
        disabled: bool,
        magnetic: bool,
        mut obs: Observations,
        calls: &mut Vec<Value>,
    ) -> Result<()> {
        let mut shadow = |ring: &mut StageRing, calls: &mut Vec<Value>| -> Result<f32> {
            let s = obs
                .shadow
                .take()
                .filter(|v| v.height.is_finite())
                .ok_or_else(|| {
                    Error::invalid("expected finite GetShadowPos result and resulting yaw")
                })?;
            calls.push(json!(["GetShadowPos", ring.position]));
            ring.yaw = s.yaw_after;
            Ok(s.height)
        };
        if mode == RingMode::Init {
            self.timer = obs.timer.take().filter(|v| *v <= 8).ok_or_else(|| {
                Error::invalid("terrain ring needs initial global collision timer (0..8)")
            })?;
            self.height = if disabled {
                NO_SHADOW
            } else {
                shadow(ring, calls)?
            };
        } else if mode == RingMode::Active {
            if magnetic {
                self.height = NO_SHADOW;
            }
            if self.height != NO_SHADOW && !disabled {
                if self.timer > 0 {
                    self.timer -= 1;
                } else {
                    self.timer = 8;
                    let reference = obs
                        .reference_position
                        .take()
                        .filter(|p| p.iter().all(|v| v.is_finite()))
                        .ok_or_else(|| {
                            Error::invalid("expected terrain range reference position")
                        })?;
                    let [x, y, z] = std::array::from_fn(|i| reference[i] - ring.position[i]);
                    if x * x + y * y + z * z < 400. * 400. {
                        let height = shadow(ring, calls)?;
                        let delta = height - self.height;
                        if delta.abs() < 8. {
                            self.height = height;
                            ring.position[1] += delta;
                        } else {
                            ring.mode = RingMode::Collected;
                            calls.push(json!(["CreateElementalTask", 10, 2, "Tobitiri"]));
                            if obs.allocated.take().ok_or_else(|| {
                                Error::invalid("expected Tobitiri allocation result")
                            })? {
                                let r = obs
                                    .random
                                    .take()
                                    .filter(|r| (0.0..=1.0).contains(r))
                                    .ok_or_else(|| {
                                        Error::invalid("expected native random sample")
                                    })?;
                                let [x, y, z] = ring.position;
                                calls.push(json!([
                                    "InitTobitiri",
                                    [x, y - 10., z],
                                    ((r * 360.) * (65536.0f32 / 360.)) as i32
                                ]));
                            }
                        }
                    }
                }
            }
        }
        if obs.timer.is_some()
            || obs.shadow.is_some()
            || obs.reference_position.is_some()
            || obs.allocated.is_some()
            || obs.random.is_some()
        {
            return Err(Error::invalid("unused terrain ring observation"));
        }
        Ok(())
    }
}
