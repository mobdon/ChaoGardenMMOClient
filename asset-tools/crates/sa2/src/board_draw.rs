//! Ordered native board presentation calls; opaque symbols and motion addresses are never executed.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DrawInput {
    pub two_player: bool,
    pub player: Option<Player>,
    pub mode: u8,
    pub position: [f32; 3],
    pub angles: [i32; 3],
    pub cached_action: i32,
    pub cached_frame: f32,
    #[serde(default)]
    pub motions: Vec<MotionRow>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub character: i32,
    pub action: i16,
    pub motion_mode: i16,
    pub frame: f32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MotionRow {
    pub index: u16,
    pub pointer: u32,
}
#[derive(Debug, PartialEq, Serialize)]
pub struct DrawPlan {
    pub selected: Option<Resources>,
    pub draw: Vec<Op>,
    pub shadow: Vec<Op>,
}
type Symbol = &'static str;
#[derive(Debug, PartialEq, Serialize)]
pub struct Resources {
    pub texture: Symbol,
    pub board: Symbol,
    pub unused_object: Symbol,
}
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    InstallMatrixCaptureCallback,
    ClearCallback,
    SetTexture(Symbol),
    PushMatrix,
    Translate([f32; 3]),
    RotateZ(i32),
    RotateX(i32),
    RotateY(i32),
    DrawRigid(Symbol),
    RotateObjectRoot(Symbol),
    CaptureMatrix,
    DrawMotion {
        object: Symbol,
        motion: u32,
        frame: f32,
    },
    DrawBlend {
        object: Symbol,
        previous: u32,
        current: u32,
        cached_frame: f32,
        zero: f32,
        frame: f32,
    },
    OnControl3d(u32),
    SetCapturedMatrix,
    DrawControlObject(Symbol),
    OffControl3d(u32),
    PopMatrix,
    ShadowBegin {
        scale: f32,
        zero: f32,
        position: [f32; 3],
    },
    ShadowEnd,
    ShadowFinish {
        kind: i32,
        position: [f32; 3],
        value: f32,
    },
}

fn resources(two_player: bool, character: i32) -> Resources {
    let (texture, board, unused_object) = match (two_player, character) {
        (false, _) => (
            "lbl_13_data_105088",
            "lbl_13_data_1057C0",
            "lbl_13_data_1060EC",
        ),
        (true, 8) => (
            "lbl_13_data_107268",
            "lbl_13_data_107F10",
            "lbl_13_data_10B56C",
        ),
        (true, 12) => (
            "lbl_13_data_107F64",
            "lbl_13_data_108C1C",
            "lbl_13_data_10C3EC",
        ),
        (true, 1) => (
            "lbl_13_data_106588",
            "lbl_13_data_107214",
            "lbl_13_data_10A72C",
        ),
        (true, _) => (
            "lbl_13_data_108C70",
            "lbl_13_data_109918",
            "lbl_13_data_10D22C",
        ),
    };
    Resources {
        texture,
        board,
        unused_object,
    }
}
fn observed(input: &DrawInput, index: i32) -> Result<u32, String> {
    if !(0..300).contains(&index) {
        return Err(format!("native motion table index {index} outside 0..300"));
    }
    let mut rows = input.motions.iter().filter(|r| i32::from(r.index) == index);
    let ptr = rows
        .next()
        .ok_or_else(|| format!("missing observation for motion row {index}"))?
        .pointer;
    if rows.next().is_some() {
        return Err(format!("duplicate observation for motion row {index}"));
    }
    Ok(ptr)
}
pub fn plan(input: &DrawInput) -> Result<DrawPlan, String> {
    use Op::*;
    let Some(player) = &input.player else {
        return Ok(DrawPlan {
            selected: None,
            draw: vec![],
            shadow: vec![],
        });
    };
    if !player.frame.is_finite()
        || !input.cached_frame.is_finite()
        || input.position.iter().any(|v| !v.is_finite())
    {
        return Err("nonfinite board display observation".into());
    }
    let selected = resources(input.two_player, player.character);
    let current = if input.mode == 1 {
        Some(observed(input, i32::from(player.action) + 24)?)
    } else {
        None
    };
    let previous = if current.is_some_and(|p| p != 0)
        && player.motion_mode == 2
        && (0x79..=0x90).contains(&input.cached_action)
    {
        Some(observed(input, input.cached_action + 24)?)
    } else {
        None
    };
    let pass = |shadow: bool| {
        let object = if shadow {
            "lbl_13_data_1063DC"
        } else {
            selected.board
        };
        let mut v = vec![
            if shadow {
                ShadowBegin {
                    scale: 1.8,
                    zero: 0.,
                    position: input.position,
                }
            } else {
                InstallMatrixCaptureCallback
            },
            SetTexture(selected.texture),
            PushMatrix,
        ];
        if current != Some(0) {
            let [x, y, z] = input.angles;
            v.extend([
                Translate(input.position),
                RotateZ(z),
                RotateX(x),
                RotateY(y),
            ]);
            match (current, previous) {
                (Some(motion), _) if player.motion_mode != 2 => v.push(DrawMotion {
                    object,
                    motion,
                    frame: player.frame,
                }),
                (Some(current), Some(previous)) => v.push(DrawBlend {
                    object,
                    previous,
                    current,
                    cached_frame: input.cached_frame,
                    zero: 0.,
                    frame: player.frame,
                }),
                _ => {
                    v.push(DrawRigid(object));
                    if !shadow {
                        v.extend([RotateObjectRoot(object), CaptureMatrix]);
                    }
                }
            }
        }
        if shadow {
            v.extend([
                PopMatrix,
                ShadowEnd,
                ShadowFinish {
                    kind: 2,
                    position: input.position,
                    value: 90.,
                },
            ]);
        } else {
            v.extend([
                OnControl3d(0x2400),
                SetCapturedMatrix,
                DrawControlObject("lbl_13_data_106534"),
                OffControl3d(0x2400),
                PopMatrix,
                ClearCallback,
            ]);
        }
        v
    };
    Ok(DrawPlan {
        draw: pass(false),
        shadow: pass(true),
        selected: Some(selected),
    })
}

#[cfg(test)]
mod tests {
    use super::{Op::*, *};

    fn input() -> DrawInput {
        DrawInput {
            two_player: true,
            player: Some(Player {
                character: 8,
                action: 121,
                motion_mode: 3,
                frame: 8.5,
            }),
            mode: 1,
            position: [1., 2., 3.],
            angles: [7, 8, 9],
            cached_action: 144,
            cached_frame: 4.5,
            motions: vec![
                MotionRow {
                    index: 145,
                    pointer: 23,
                },
                MotionRow {
                    index: 168,
                    pointer: 0,
                },
            ],
        }
    }

    #[test]
    fn resources_follow_native_selector() {
        for (two, id, t, b, unused) in [
            (false, 8, "105088", "1057C0", "1060EC"),
            (true, 8, "107268", "107F10", "10B56C"),
            (true, 12, "107F64", "108C1C", "10C3EC"),
            (true, 1, "106588", "107214", "10A72C"),
            (true, -1, "108C70", "109918", "10D22C"),
        ] {
            let r = resources(two, id);
            for (actual, suffix) in [(r.texture, t), (r.board, b), (r.unused_object, unused)] {
                assert_eq!(actual, format!("lbl_13_data_{suffix}"));
            }
        }
    }

    #[test]
    fn rigid_calls_keep_native_matrix_order_and_shadow_helpers() {
        let mut i = input();
        i.mode = 2;
        i.motions.clear();
        let p = plan(&i).unwrap();
        let b = "lbl_13_data_107F10";
        let t = "lbl_13_data_107268";
        assert_eq!(
            p.draw,
            vec![
                InstallMatrixCaptureCallback,
                SetTexture(t),
                PushMatrix,
                Translate(i.position),
                RotateZ(9),
                RotateX(7),
                RotateY(8),
                DrawRigid(b),
                RotateObjectRoot(b),
                CaptureMatrix,
                OnControl3d(0x2400),
                SetCapturedMatrix,
                DrawControlObject("lbl_13_data_106534"),
                OffControl3d(0x2400),
                PopMatrix,
                ClearCallback
            ]
        );
        assert_eq!(
            p.shadow,
            vec![
                ShadowBegin {
                    scale: 1.8,
                    zero: 0.,
                    position: i.position
                },
                SetTexture(t),
                PushMatrix,
                Translate(i.position),
                RotateZ(9),
                RotateX(7),
                RotateY(8),
                DrawRigid("lbl_13_data_1063DC"),
                PopMatrix,
                ShadowEnd,
                ShadowFinish {
                    kind: 2,
                    position: i.position,
                    value: 90.
                }
            ]
        );
    }

    #[test]
    fn null_row_preserves_suffix_and_missing_player_skips_both_callbacks() {
        let mut i = input();
        i.motions[0].pointer = 0;
        let p = plan(&i).unwrap();
        assert_eq!(
            &p.draw[3..],
            &[
                OnControl3d(0x2400),
                SetCapturedMatrix,
                DrawControlObject("lbl_13_data_106534"),
                OffControl3d(0x2400),
                PopMatrix,
                ClearCallback
            ]
        );
        assert_eq!(
            &p.shadow[3..],
            &[
                PopMatrix,
                ShadowEnd,
                ShadowFinish {
                    kind: 2,
                    position: i.position,
                    value: 90.
                }
            ]
        );
        i.player = None;
        i.motions.clear();
        let p = plan(&i).unwrap();
        assert!(p.selected.is_none() && p.draw.is_empty() && p.shadow.is_empty());
    }

    #[test]
    fn motion_and_blend_use_observed_frames_and_inclusive_range() {
        let mut i = input();
        let b = "lbl_13_data_107F10";
        assert_eq!(
            plan(&i).unwrap().draw[7],
            DrawMotion {
                object: b,
                motion: 23,
                frame: 8.5
            }
        );
        i.player.as_mut().unwrap().motion_mode = 2;
        for (action, ptr) in [(121, 23), (144, 0)] {
            i.cached_action = action;
            let p = plan(&i).unwrap();
            for (ops, object) in [(p.draw, b), (p.shadow, "lbl_13_data_1063DC")] {
                assert_eq!(
                    ops[7],
                    DrawBlend {
                        object,
                        current: 23,
                        previous: ptr,
                        cached_frame: 4.5,
                        zero: 0.,
                        frame: 8.5
                    }
                );
            }
        }
        for action in [120, 145, i32::MAX, i32::MIN] {
            i.cached_action = action;
            assert_eq!(plan(&i).unwrap().draw[7], DrawRigid(b));
        }
    }

    #[test]
    fn unknown_rows_and_invalid_signed_indices_fail_closed() {
        let mut i = input();
        i.player.as_mut().unwrap().motion_mode = 2;
        i.motions.pop();
        assert!(plan(&i)
            .unwrap_err()
            .contains("missing observation for motion row 168"));
        i.motions.push(i.motions[0].clone());
        assert!(plan(&i)
            .unwrap_err()
            .contains("duplicate observation for motion row 145"));
        for (action, index) in [(-24, 0), (275, 299)] {
            i.player.as_mut().unwrap().action = action;
            i.motions = vec![MotionRow { index, pointer: 0 }];
            assert!(plan(&i).is_ok());
        }
        for action in [-25, 276] {
            i.player.as_mut().unwrap().action = action;
            assert!(plan(&i).unwrap_err().contains("outside 0..300"));
        }
    }
}
