//! The default player camera after INIT (E6; ENGINE-MAP.md section 6):
//! mode 0xD "CARMEL" (fn_800C86A4), the follow routine it runs every frame
//! (fn_800C9CA8, mode 3's routine) and the camera exec steps around them
//! that the follow routine reads back next frame (fn_800C7590 case 0,
//! fn_800C7464, fn_800C6B38). Ported from main.dol for viewport 0.
//!
//! Not ported (recorded in [`Follow::unported`], not run): the treasure
//! hunters' variant fn_800CA864, the action-40 mode switch (fn_800C3958,
//! fn_800C91E4), the adjust-mode routines the adjust stack points at, and
//! the exec's other steps (fn_800C6860, fn_800C718C, fn_800C4964,
//! fn_800C14AC, fn_800C7358, fn_800C4038). Line tests (fn_800C3214) and
//! the search directions (0x801CBC40) come from a [`FollowHost`].
use sa2_asset_tools::{
    msl_math::{atan2f, gekko_frsqrte},
    ninja_matrix::{rotate_by_angles, Axis, MatrixStack, Vec3},
    ninja_trig::{fctiwz, fctiwz_f64, fmadds, fnmsubs, nj_cos, nj_sin},
    ninja_vector::{scalor, unit_vector},
    player_path::floorf,
    player_turn::{adjust_angle, diff_angle, sub_angle},
};

// .sdata2 constants (r2 = 0x803B6520).
const BAMS: f64 = f64::from_bits(0x40C4_5F30_B217_88BB); // r2-0x6498, r2-0x6240: 65536 / 2pi
const SQRT_HALF: f64 = 0.5; // r2-0x6488
const SQRT_THREE: f64 = 3.0; // r2-0x6480
const NEAR: f32 = 50.0; // r2-0x62E8
const NINETIETH: f32 = f32::from_bits(0x3C36_0B61); // r2-0x62E4
const TRAIL_SPAN: f32 = 32.0; // r2-0x62E0
const ONE: f32 = 1.0; // r2-0x62DC, r2-0x6254, r2-0x6420
const NUDGE: f32 = 0.01; // r2-0x62D4, r2-0x6234
const CLOSE: f32 = 35.0; // r2-0x62D0, r2-0x6264
const BLEND_FRAMES: f32 = 60.0; // r2-0x62CC
const FAR_AWAY: f32 = 100_000.0; // r2-0x6260
const STILL: f32 = 2.0; // r2-0x625C
const LEAD_DIV: f32 = 5.0; // r2-0x6258
const HALF: f32 = 0.5; // r2-0x6250
const LEAD: f32 = 12.0; // r2-0x6248
const EASE: f32 = 0.1; // r2-0x6244
const TRIGGER_TURN: f32 = 546.0; // r2-0x6238
const TRIGGER_SCALE: f32 = 0.01; // r13-0x7D80 (.sdata, never written here)
const FAR: f32 = 50.0; // r2-0x6230
const FAR_DAMP: f32 = 0.9; // r2-0x622C
const FAR_MAX: f32 = 70.0; // r2-0x6228
const NEAR_GROW: f32 = 1.06; // r2-0x6224
const NEAR_MIN: f32 = 20.0; // r2-0x6220
const RING_SQ: f32 = 900.0; // r2-0x621C
const RING: f32 = 30.0; // r2-0x6218
/// 0x80175628: the start offset mode 3 turns behind the player.
const START_OFFSET: Vec3 = [0.0, 0.0, -42.5];
/// The pitch mode 3 eases toward (-20 degrees).
const PITCH: i32 = -0xE38;
const TRAIL: usize = 50;

/// What the camera reads from viewport 0's player.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Player {
    /// twp +0x14.
    pub pos: Vec3,
    /// twp +0x8, +0xC, +0x10.
    pub ang: [i32; 3],
    /// Player work +0x13C.
    pub height: f32,
    /// twp byte +0 (the action).
    pub action: i8,
    /// Player work byte +1 (`GetPlayerCharacter`).
    pub character: i8,
}

/// Per frame inputs besides the player.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// `GetStageNum`.
    pub stage: i16,
    /// G (0x801CC168): byte +0 picks buttons over triggers, +0x7C is the
    /// frame counter.
    pub g: &'a [u8],
    /// Viewport 0's pad copy (0x801E5438): held word +0x8, triggers +0x18
    /// and +0x1A.
    pub pad: &'a [u8],
    /// 0x801E776C: player 2's twp is set.
    pub second_player: bool,
    /// Player 0's history ring (r13-0x7910 index, r13-0x7908 positions).
    pub history_index: u8,
    pub history_pos: &'a [Vec3],
}

/// The line test fn_800C3214 and the search directions table.
pub trait FollowHost {
    /// fn_800C3214 with flags 0x2002 and radius 0: non-zero when something
    /// lies between `from` and `to`.
    fn line(&mut self, from: Vec3, to: Vec3) -> i32;
    /// Row `i` of 0x801CBC40 (60 rows of two angles).
    fn search_angles(&mut self, i: i32) -> Option<[i32; 2]>;
}

/// The view block 0x801FF3E4.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewBlock {
    /// +0: how fn_800C7590 derives the rest (0: from position and target).
    pub kind: i32,
    pub w4: i32,
    pub w8: i32,
    /// +0xC.
    pub pos: Vec3,
    /// +0x18: pitch, yaw, roll.
    pub ang: [i32; 3],
    /// +0x24.
    pub dir: Vec3,
    /// +0x30.
    pub target: Vec3,
    /// +0x3C.
    pub dist: f32,
}

/// Viewport 0's camera block from +0x194: last frame's view.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Block {
    pub view: ViewBlock,
    /// +0x1C4: this frame's move.
    pub vel: Vec3,
}

/// The mode slot's 0x80-byte work (+0x26C), as CARMEL and the follow
/// routine share it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SlotWork {
    /// +0: the turned angle.
    pub ang: i32,
    /// +4: turned by the player this frame.
    pub turned: i8,
    /// +5: the player is standing still.
    pub idle: i8,
    /// +6: frames without moving.
    pub still: u8,
    /// +7.
    pub b7: i8,
    /// +8: where the player stood.
    pub anchor: Vec3,
    pub w14: i32,
    /// +0x18: CARMEL's state.
    pub state: i32,
    /// +0x1C.
    pub count: i32,
    /// +0x20: the trail counter when state 1 began.
    pub moves: i32,
    /// +0x34, +0x40 (pitch), +0x44 (yaw), +0x48, +0x4C (search row).
    pub v34: Vec3,
    pub pitch: i32,
    pub yaw: i32,
    pub w48: i32,
    pub search: i32,
    /// +0x50.
    pub blend: i32,
    /// +0x58: search radius.
    pub radius: f32,
    /// +0x5C: the spot the search found.
    pub found: Vec3,
}

/// Viewport 0's follow camera state.
#[derive(Debug, Clone, PartialEq)]
pub struct Follow {
    pub view: ViewBlock,
    pub block: Block,
    pub work: SlotWork,
    /// Slot 0's frame counter (+0x260).
    pub frames: u32,
    /// 0x803ADB1C: camera execs so far.
    pub execs: i32,
    /// r13-0x7790: trail points recorded.
    pub moves: i32,
    /// 0x80208C30 and its index 0x80209590.
    pub trail: [Vec3; TRAIL],
    pub trail_index: i32,
    /// 0x80208B30: the last 16 actions, by frame counter.
    pub actions: [i32; 16],
    /// 0x8021C5C0: the action CARMEL saw last frame.
    pub last_action: i32,
    /// r13-0x7798: turning by the player is off.
    pub turn_lock: i32,
    /// The adjust stack (+0x1398): the mode put in each slot.
    pub adjust: [u32; 16],
    /// Engine calls this frame that are not ported: (address, args).
    pub unported: Vec<(u32, Vec<u32>)>,
}

impl Default for Follow {
    fn default() -> Self {
        Self {
            view: ViewBlock::default(),
            block: Block::default(),
            work: SlotWork::default(),
            frames: 0,
            execs: 0,
            moves: 0,
            trail: [[0.0; 3]; TRAIL],
            trail_index: 0,
            actions: [0; 16],
            last_action: 0,
            turn_lock: 0,
            adjust: [0; 16],
            unported: vec![],
        }
    }
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn raised(p: &Player) -> Vec3 {
    [p.pos[0], p.pos[1] + p.height, p.pos[2]]
}

fn be16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2)
        .map_or(0, |x| u16::from_be_bytes([x[0], x[1]]))
}

fn be32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

/// fn_800C55BC: a vector's (pitch, yaw) in BAMS.
pub fn vector_angles(v: Vec3) -> (i32, i32) {
    let yaw = fctiwz_f64(BAMS * atan2f(-v[0], -v[2]) as f64);
    let mut h = v[0] * v[0] + v[2] * v[2];
    if h > 0.0 {
        // Three Newton steps on frsqrte, in double, then x * (1/sqrt x).
        let x = h as f64;
        let mut e = gekko_frsqrte(x);
        for _ in 0..3 {
            let sq = e * e;
            let half = SQRT_HALF * e;
            e = half * -x.mul_add(sq, -SQRT_THREE);
        }
        h = (x * e) as f32;
    }
    let pitch = fctiwz_f64(BAMS * atan2f(v[1], h) as f64);
    (pitch, yaw)
}

/// fn_800C549C: (0, 0, -1) turned by yaw, then pitch.
fn angles_dir(stack: &mut MatrixStack, pitch: i32, yaw: i32) -> Vec3 {
    stack.push_unit();
    stack.rotate(Axis::Y, yaw);
    stack.rotate(Axis::X, pitch);
    let v = stack.calc_vector([0.0, 0.0, -1.0]);
    stack.pop();
    v
}

/// The camera exec is skipped for stages 70 and 71 and without a player
/// (fn_800C6A90 and the same test in fn_800C6B38 and fn_800C6CDC).
fn off(stage: i16) -> bool {
    (70..72).contains(&stage)
}

impl Follow {
    /// fn_800C3BDC: the top mode slot (CARMEL's slot 0 once INIT dropped).
    const TOP: u32 = 0;

    /// fn_800C382C(vp, slot, mode) on the adjust stack.
    fn set_adjust(&mut self, slot: u32, mode: u32) {
        if let Some(a) = self.adjust.get_mut(slot as usize) {
            *a = mode;
        }
    }

    /// fn_800C6AFC(vp, n): the trail point `n` back.
    fn trail_back(&self, n: i32) -> Vec3 {
        let k = self.trail_index.wrapping_sub(n);
        // shortcut: the game adds 50 once; n stays in 1..=33 here, so the
        // wrap never needs more.
        let k = if k < 0 { k + TRAIL as i32 } else { k };
        usize::try_from(k)
            .ok()
            .and_then(|k| self.trail.get(k))
            .copied()
            .unwrap_or([0.0; 3])
    }

    /// fn_800C6CDC: INIT's hand-over fills the trail with the player.
    pub fn reset_trail(&mut self, stage: i16, p: &Player) {
        if off(stage) {
            return;
        }
        self.trail = [p.pos; TRAIL];
        self.trail_index = 0;
    }

    /// fn_800C5D54: the line from the player (raised, pulled back along
    /// its facing) to `to`.
    fn blocked(
        &self,
        host: &mut dyn FollowHost,
        stack: &mut MatrixStack,
        p: &Player,
        to: Vec3,
    ) -> i32 {
        let start = raised(p);
        let (mut face, _) = unit_vector(rotate_by_angles(stack, [0, p.ang[1], 0], [1.0, 0.0, 0.0]));
        face[1] = 0.0;
        host.line(sub(start, face), to)
    }

    /// fn_800C5B78: try one search direction around the player; 1 when its
    /// spot is clear (then in `found`).
    fn search(&mut self, host: &mut dyn FollowHost, stack: &mut MatrixStack, p: &Player) -> i32 {
        let w = &self.work;
        let Some(row) = host.search_angles(w.search) else {
            self.unported.push((0x801C_BC40, vec![w.search as u32]));
            return 0;
        };
        let d = angles_dir(
            stack,
            w.pitch.wrapping_add(row[0]),
            w.yaw.wrapping_add(row[1]),
        );
        let d = d.map(|c| c * w.radius);
        let spot = [p.pos[0] + d[0], p.pos[1] + d[1], p.pos[2] + d[2]];
        if self.blocked(host, stack, p, spot) == 0 {
            self.work.found = spot;
            return 1;
        }
        self.work.search += 1;
        if self.work.search >= 60 {
            self.work.search = 0;
        }
        0
    }

    /// One CARMEL call (fn_800C86A4) for viewport 0.
    pub fn carmel(&mut self, host: &mut dyn FollowHost, p: &Player, inp: &Inputs<'_>) {
        if off(inp.stage) {
            return;
        }
        let mut stack = MatrixStack::new();
        if self.frames == 0 {
            self.work.state = 2;
        }
        let mut state = self.work.state;
        if state == 0 {
            self.work.b7 = 1;
            let d = ((self.moves.wrapping_sub(self.work.moves)) as f32).abs();
            if d <= NEAR {
                self.work.state = 3;
                state = 3;
            } else {
                self.work.state = 1;
                self.work.w14 = 0x78;
                state = 1;
            }
        }
        match state {
            1 => self.toward_trail(host, &mut stack, p),
            2 => {
                self.work.count = 0;
                self.follow(p, inp);
                if (4..6).contains(&p.character)
                    && self.work.turned == 0
                    && self.last_action != 69
                    && p.action != 69
                    && !(65..=68).contains(&p.action)
                    && self.blocked(host, &mut stack, p, self.block.view.pos) != 0
                {
                    self.work.state = 0;
                    self.set_adjust(Self::TOP, 0);
                }
                self.last_action = p.action as i32;
            }
            3..=5 => self.around(host, &mut stack, p, state),
            _ => {}
        }
    }

    /// CARMEL state 1: ease back along the trail toward the player.
    fn toward_trail(&mut self, host: &mut dyn FollowHost, stack: &mut MatrixStack, p: &Player) {
        self.work.moves = self.moves;
        let t = NINETIETH * self.work.count as f32;
        let back = fnmsubs(TRAIL_SPAN, t, TRAIL_SPAN);
        let i = fctiwz(ONE + floorf(back));
        let a = self.trail_back(i);
        let b = self.trail_back(i.wrapping_add(1));
        let frac = back - i as f32;
        let last = self.block.view.pos;
        let mut cam = [0.0; 3];
        for k in 0..3 {
            let on = fmadds(b[k] - a[k], frac, a[k]);
            cam[k] = fmadds(t, on - last[k], last[k]);
        }
        let (dx, dz) = (cam[0] - p.pos[0], cam[2] - p.pos[2]);
        if dx * dx + dz * dz < ONE {
            let dx = if dx == 0.0 && dz == 0.0 {
                dx + NUDGE
            } else {
                dx
            };
            let (u, _) = unit_vector([dx, 0.0, dz]);
            cam[0] = fmadds(ONE, u[0], p.pos[0]);
            cam[2] = fmadds(ONE, u[2], p.pos[2]);
        }
        self.view.pos = cam;
        self.view.target = raised(p);
        if scalor(sub(cam, raised(p))) < CLOSE {
            self.work.state = 2;
        }
        self.work.count += 1;
        if self.work.count >= 90 {
            self.work.state = 2;
        }
        self.view.kind = 0;
        self.view.w4 = 0;
        self.view.w8 = 0;
        if self.work.state == 2 && self.blocked(host, stack, p, self.view.pos) != 0 {
            self.work.state = 3;
        }
    }

    /// CARMEL states 3 to 5: look for a clear spot around the player, then
    /// move there over 30 frames.
    fn around(
        &mut self,
        host: &mut dyn FollowHost,
        stack: &mut MatrixStack,
        p: &Player,
        state: i32,
    ) {
        if state == 3 {
            self.work.radius = CLOSE;
            self.work.v34 = sub(self.view.pos, raised(p));
            let (pitch, yaw) = vector_angles(self.work.v34);
            self.work.pitch = pitch.wrapping_neg();
            self.work.yaw = yaw;
            self.work.w48 = 0;
            self.work.search = 0;
            self.work.state = 4;
        }
        if state <= 4 {
            if self.search(host, stack, p) == 0 {
                self.view.target = raised(p);
                return;
            }
            self.work.state = 5;
            self.work.blend = 0;
        }
        let t = self.work.blend as f32;
        let last = self.block.view.pos;
        for k in 0..3 {
            self.view.pos[k] = t * (self.work.found[k] - last[k]) / BLEND_FRAMES + last[k];
        }
        self.view.target = raised(p);
        self.work.blend += 1;
        if self.work.blend >= 30 {
            self.work.state = 2;
        }
    }

    /// fn_800C9CA8 (mode 3's routine): follow the player.
    pub fn follow(&mut self, p: &Player, inp: &Inputs<'_>) {
        let s = inp.stage;
        let checked = if s < 27 {
            s != 8 && !(25..27).contains(&s)
        } else {
            s != 59
        };
        let hunter = checked
            && (4..6).contains(&p.character)
            && (p.action == 69 || (65..=68).contains(&p.action) || inp.second_player);
        if hunter {
            self.unported.push((0x800C_A864, vec![0, Self::TOP]));
            return;
        }
        let w = &mut self.work;
        if self.frames == 0 {
            w.turned = 0;
            w.anchor = [FAR_AWAY; 3];
            w.idle = 0;
            w.still = 0;
            w.b7 = 0;
            w.w14 = 0;
        }
        if w.w14 < 0 {
            w.w14 = 0;
        }
        let d = scalor(sub(p.pos, w.anchor));
        if w.idle != 0 {
            if d > STILL {
                w.anchor = p.pos;
                w.still = 0;
                w.idle = 0;
            }
        } else if d < STILL {
            w.still = w.still.wrapping_add(1);
            if w.still > 60 {
                w.idle = 1;
            }
        } else {
            w.anchor = p.pos;
        }
        let mut aim = self.block.view.target;
        if w.idle == 0 {
            aim = raised(p);
            if self.action_back(inp, 5) != 40 {
                let k = inp.history_index.wrapping_sub(5) as usize;
                let mut was = inp.history_pos.get(k).copied().unwrap_or(p.pos);
                was[1] += p.height;
                let mut v = sub(aim, was).map(|c| c / LEAD_DIV);
                let len = scalor(v);
                if len < ONE {
                    if len < HALF {
                        v = [0.0; 3];
                    } else {
                        let k = STILL * (len - HALF);
                        v = v.map(|c| c * k);
                    }
                }
                for i in 0..3 {
                    aim[i] = fmadds(LEAD, v[i], aim[i]);
                }
            }
        }
        let prev = self.block.view;
        if self.execs < 5 {
            self.view.target = aim;
        } else {
            for i in 0..3 {
                self.view.target[i] = fmadds(EASE, aim[i] - prev.target[i], prev.target[i]);
            }
        }
        self.view.pos = prev.pos;
        if self.execs < 5 {
            // shortcut: njPushMatrix copies whatever matrix is current; the
            // unit matrix is assumed. Only the first five execs reach here.
            let mut stack = MatrixStack::new();
            stack.push_unit();
            stack.rotate(Axis::Z, 0);
            stack.rotate(Axis::X, PITCH);
            stack.rotate(Axis::Y, p.ang[1].wrapping_neg());
            let v = stack.calc_vector(START_OFFSET);
            stack.pop();
            self.view.pos = [p.pos[0] + v[0], p.pos[1] - v[1], p.pos[2] + v[2]];
        }
        if self.work.b7 == 0 {
            let mut a = sub_angle(PITCH, self.block.view.ang[0]) as i16;
            let mut lift = if (6..8).contains(&p.character) {
                LEAD_DIV
            } else {
                ONE
            };
            if a < 0 {
                a = a.wrapping_neg();
            }
            if a > 1820 {
                let pitch = adjust_angle(self.block.view.ang[0], PITCH, 0xB6);
                self.block.view.ang[0] = pitch;
                if diff_angle(PITCH, pitch) < 1820 {
                    lift = ONE;
                }
                self.view.pos[1] =
                    fmadds(lift, nj_sin(pitch.wrapping_add(0xE38)), self.view.pos[1]);
            }
        }
        if self.turn_lock == 0 {
            self.turn(p, inp);
        }
        self.keep_distance(p);
        self.view.kind = 0;
        self.view.ang[2] = 0;
        self.view.w4 = i32::from(self.execs >= 5);
        self.view.w8 = 0;
        if p.action as u8 == 40 {
            self.unported
                .push((0x800C_3958, vec![0, Self::TOP, 0x800C_9B30]));
            self.set_adjust(Self::TOP, 8);
            self.unported.push((0x800C_91E4, vec![0, 0xB4]));
            self.frames = u32::MAX;
        }
    }

    /// fn_800C6A60(vp, n): the action `n` frames back.
    fn action_back(&self, inp: &Inputs<'_>, n: u32) -> i32 {
        let frame = be32(inp.g, 0x7C);
        self.actions[(frame.wrapping_sub(n) & 15) as usize]
    }

    /// The player turning the camera: triggers, or the C buttons when G+0
    /// is set.
    fn turn(&mut self, p: &Player, inp: &Inputs<'_>) {
        let dist = scalor(sub(self.view.pos, p.pos));
        let mut ang = if self.work.turned != 0 {
            self.work.ang
        } else {
            let dz = self.view.pos[2] - p.pos[2];
            let dx = self.view.pos[0] - p.pos[0];
            fctiwz_f64(BAMS * atan2f(dz, dx) as f64)
        };
        self.work.turned = 0;
        if inp.g.first().copied().unwrap_or(0) == 0 {
            let right = i32::from(be16(inp.pad, 0x1A)) - 30;
            if right > 0 {
                ang = fctiwz(fmadds(
                    TRIGGER_TURN,
                    TRIGGER_SCALE * right as f32,
                    ang as f32,
                ));
                self.turned_to(ang);
            }
            let left = i32::from(be16(inp.pad, 0x18)) - 30;
            if left > 0 {
                ang = fctiwz(fnmsubs(
                    TRIGGER_TURN,
                    TRIGGER_SCALE * left as f32,
                    ang as f32,
                ));
                self.turned_to(ang);
            }
        } else {
            let held = be32(inp.pad, 0x8);
            if held & 0x20000 != 0 {
                ang = ang.wrapping_add(0x222);
                self.turned_to(ang);
            }
            if held & 0x10000 != 0 {
                ang = ang.wrapping_sub(0x222);
                self.turned_to(ang);
            }
        }
        if self.work.turned != 0 {
            let last = self.block.view.target[1];
            self.view.target[1] = fmadds(EASE, (p.height + p.pos[1]) - last, last);
            self.view.pos[0] = fmadds(dist, nj_cos(ang), p.pos[0]);
            self.view.pos[2] = fmadds(dist, nj_sin(ang), p.pos[2]);
        }
    }

    fn turned_to(&mut self, ang: i32) {
        self.work.ang = ang;
        self.work.turned = 1;
        self.set_adjust(Self::TOP, 0);
    }

    /// Keep the camera between 20 and 70 units from the player and at least
    /// 30 away on the ground plane.
    fn keep_distance(&mut self, p: &Player) {
        let mut d = sub(p.pos, self.view.pos);
        if d == [0.0; 3] {
            d[0] += NUDGE;
        }
        let mut dist = scalor(d);
        let (d, _) = unit_vector(d);
        let w = &mut self.work;
        if dist > FAR {
            dist = fmadds(FAR_DAMP, dist - FAR, FAR);
            if w.w14 == 0 && dist > FAR_MAX {
                dist = FAR_MAX;
            }
            w.b7 = 0;
        } else if dist < CLOSE {
            dist *= NEAR_GROW;
            if w.turned == 0 && w.w14 == 0 && dist < NEAR_MIN {
                dist = NEAR_MIN;
            }
        }
        for i in 0..3 {
            self.view.pos[i] = fnmsubs(d[i], dist, p.pos[i]);
        }
        let hx = self.view.pos[0] - p.pos[0];
        let hz = self.view.pos[2] - p.pos[2];
        if hx * hx + hz * hz < RING_SQ {
            let hx = if hx == 0.0 && hz == 0.0 {
                hx + NUDGE
            } else {
                hx
            };
            let (n, _) = unit_vector([hx, 0.0, hz]);
            self.view.pos[0] = fmadds(RING, n[0], p.pos[0]);
            self.view.pos[2] = fmadds(RING, n[2], p.pos[2]);
        }
    }

    /// fn_800C7590 case 0: direction, distance and angles from the camera
    /// position and target.
    pub fn derive(&mut self) {
        if self.view.kind != 0 {
            // shortcut: only kind 0 is ported; INIT and the follow routine
            // both set it.
            self.unported
                .push((0x800C_7590, vec![self.view.kind as u32]));
            return;
        }
        self.view.dir = sub(self.view.target, self.view.pos);
        self.view.dist = scalor(self.view.dir);
        let (pitch, yaw) = vector_angles(self.view.dir);
        self.view.ang[0] = pitch;
        self.view.ang[1] = yaw;
    }

    /// fn_800C7464: keep this frame's view in the block, with its move.
    pub fn keep(&mut self) {
        let old = self.block.view.pos;
        self.block.view = self.view;
        self.block.vel = sub(self.view.pos, old);
    }

    /// fn_800C6B38: add a trail point when the player moved more than one
    /// unit since the last, and note the action.
    pub fn record(&mut self, p: &Player, inp: &Inputs<'_>) {
        if off(inp.stage) {
            return;
        }
        let last = self.trail_back(1);
        let d = [
            p.pos[0] - last[0],
            p.height + (p.pos[1] - last[1]),
            p.pos[2] - last[2],
        ];
        if scalor(d) > ONE {
            self.moves = self.moves.wrapping_add(1);
            if let Some(t) = usize::try_from(self.trail_index)
                .ok()
                .and_then(|i| self.trail.get_mut(i))
            {
                *t = raised(p);
            }
            self.trail_index += 1;
            if self.trail_index >= TRAIL as i32 {
                self.trail_index = 0;
            }
        }
        let frame = be32(inp.g, 0x7C);
        self.actions[(frame & 15) as usize] = p.action as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Open;
    impl FollowHost for Open {
        fn line(&mut self, _: Vec3, _: Vec3) -> i32 {
            0
        }
        fn search_angles(&mut self, _: i32) -> Option<[i32; 2]> {
            Some([0, 0])
        }
    }

    fn inputs<'a>(g: &'a [u8], pad: &'a [u8], hist: &'a [Vec3]) -> Inputs<'a> {
        Inputs {
            stage: 11,
            g,
            pad,
            second_player: false,
            history_index: 0,
            history_pos: hist,
        }
    }

    #[test]
    fn vector_angles_quarter_turns() {
        // Looking down -Z is yaw 0; looking down -X is a quarter turn.
        assert_eq!(vector_angles([0.0, 0.0, -1.0]).1, 0);
        assert_eq!(vector_angles([-1.0, 0.0, 0.0]).1, 0x4000);
        let (pitch, _) = vector_angles([0.0, 1.0, -1.0]);
        assert!((pitch - 0x2000).abs() <= 1, "{pitch:#x}");
    }

    #[test]
    fn follow_settles_between_thirty_and_seventy() {
        let (g, pad, hist) = (vec![0u8; 0xF0], vec![0u8; 0x34], vec![[0.0; 3]; 0x100]);
        let mut f = Follow::default();
        f.block.view.pos = [-60.0, 30.0, 0.0];
        f.block.view.target = [0.0, 10.0, 0.0];
        let p = Player {
            height: 10.0,
            character: 7,
            ..Player::default()
        };
        for _ in 0..200 {
            f.view = f.block.view;
            f.carmel(&mut Open, &p, &inputs(&g, &pad, &hist));
            f.derive();
            f.keep();
            f.record(&p, &inputs(&g, &pad, &hist));
            f.frames += 1;
            f.execs += 1;
        }
        assert!(f.unported.is_empty());
        let d = scalor(sub(f.view.pos, p.pos));
        assert!((30.0..=70.0).contains(&d), "{d}");
        let [hx, _, hz] = sub(f.view.pos, p.pos);
        assert!(hx * hx + hz * hz >= RING_SQ - 1.0);
        assert_eq!(f.view.target, [0.0, 10.0, 0.0]);
    }

    #[test]
    fn right_trigger_turns_the_camera() {
        let (g, mut pad, hist) = (vec![0u8; 0xF0], vec![0u8; 0x34], vec![[0.0; 3]; 0x100]);
        let mut f = Follow::default();
        f.block.view.pos = [-40.0, 20.0, 0.0];
        f.view = f.block.view;
        f.frames = 1;
        f.execs = 10;
        pad[0x1B] = 130;
        let p = Player::default();
        f.follow(&p, &inputs(&g, &pad, &hist));
        assert_eq!(f.work.turned, 1);
        assert_eq!(f.adjust[0], 0);
        // From angle 0x8000 (camera on -X) by 546 * 0.01 * 100.
        assert_eq!(f.work.ang, 0x8000 + 546);
    }

    #[test]
    fn trail_records_moves_over_one_unit() {
        let (g, pad, hist) = (vec![0u8; 0xF0], vec![0u8; 0x34], vec![[0.0; 3]; 0x100]);
        let mut f = Follow::default();
        let mut p = Player::default();
        f.reset_trail(11, &p);
        p.pos = [0.5, 0.0, 0.0];
        f.record(&p, &inputs(&g, &pad, &hist));
        assert_eq!((f.moves, f.trail_index), (0, 0));
        p.pos = [3.0, 0.0, 0.0];
        f.record(&p, &inputs(&g, &pad, &hist));
        assert_eq!((f.moves, f.trail_index), (1, 1));
        assert_eq!(f.trail[0], [3.0, 0.0, 0.0]);
    }
}
