//! Native Chao Karate rules, ported from the GameCube SA2B (GSNE8P)
//! `ChaoStgKarate.rel` (module 72, SHA-1 7379b87d640a23b58b2ea12181bb74704bd9718a)
//! and `ChaoMain.rel` (module 60). Addresses are ChaoStgKarate `.text` offsets
//! unless marked `CM` (ChaoMain) or `main` (main.dol). `tests/chao_karate.rs`
//! checks every routine here bit for bit against the module's own PowerPC.
//!
//! The fighter record is the karate work block at task+0x40: +2 side, +4 flags,
//! +6 opponent kind, +0x20.. the float parameters below. The state record is
//! task+0x34: +0 state, +1 step, +3 frame counter (0..59), +4 flags, +6 timer.
//!
//! Only rules are here. Motion lengths, movement, the ring-out boundary and the
//! cheering input are not ported (see the trace note).

/// Native constants. Values are the module's own `.data`/`.rodata` words.
pub mod native {
    /// main 0x8010DC88 `rand`: next = next * 0x41C64E6D + 0x3039, (next >> 16) & 0x7FFF.
    pub const RAND_MUL: u32 = 0x41C6_4E6D;
    pub const RAND_ADD: u32 = 0x3039;
    /// rodata 0x45C: 1/32768, the rand() scale every routine uses.
    pub const RAND_SCALE: f32 = 3.051_757_8e-5;
    /// CM ChaoGlobal+0x126 `SkillWalk` (ChaoMain .data 0x2A50 + 0x126).
    pub const SKILL_WALK: u16 = 150;
    /// fn_72_A390 0xA3A0: round clock in seconds (match manager +0xC).
    pub const ROUND_SECONDS: i16 = 90;
    /// fn_72_A22C 0xA300: the clock loses one second every 60 frames.
    pub const FRAMES_PER_CLOCK_SECOND: u16 = 60;
    /// fn_72_5020 0x50B8: 5 rounds per class; descriptor row = class * 5 + round + 2.
    pub const ROUNDS_PER_CLASS: u8 = 5;
    pub const FIRST_OPPONENT_ROW: usize = 2;
    /// Emblem bytes save+0x3231+class, fn_72_5D48 0x5E4C (4 classes).
    pub const CLASSES: u8 = 4;
}

/// main 0x8010DC88 `rand` (MSL). `next` starts at 1 (.sdata 0x803AD6A0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MslRand {
    pub next: u32,
}

impl Default for MslRand {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl MslRand {
    pub fn rand(&mut self) -> i32 {
        self.next = self
            .next
            .wrapping_mul(native::RAND_MUL)
            .wrapping_add(native::RAND_ADD);
        ((self.next >> 16) & 0x7FFF) as i32
    }
}

/// rand() / 32768 as every routine computes it (exact in single precision).
fn unit(r: i32) -> f32 {
    r as f32 * native::RAND_SCALE
}

/// A source of native rand() values (0..=32767).
pub trait Rand {
    fn rand(&mut self) -> i32;
}

impl Rand for MslRand {
    fn rand(&mut self) -> i32 {
        MslRand::rand(self)
    }
}

impl<F: FnMut() -> i32> Rand for F {
    fn rand(&mut self) -> i32 {
        self()
    }
}

/// Fighter flags (work +4).
pub mod flag {
    /// 0x112FC: the fighter came from the opponent table (its skills are jittered).
    pub const CPU: u16 = 4;
    /// 0x11A18: a child Chao whose run skill is below `SkillWalk`.
    pub const SLOW_CHILD: u16 = 1;
    /// 0x11A34: Chao param +0xD6 is set.
    pub const D6: u16 = 8;
}

/// State-record flags (task+0x34 +4).
pub mod state_flag {
    /// 0x1035C: knocked out (HP fell below zero).
    pub const KNOCKED_OUT: u16 = 0x1000;
    /// fn_72_EDE8: zeal ran out or the fighter gave up; it falls into state 7.
    pub const GIVE_UP: u16 = 0x800;
    /// fn_72_EA20 0xEBCC: the last attack connected.
    pub const HIT: u16 = 0x200;
    /// fn_72_EA20 0xEAA4: a hit check ran.
    pub const CHECKED: u16 = 0x100;
    /// fn_72_FE9C 0xFF4C: the attack in progress connected.
    pub const ATTACK_HIT: u16 = 0x400;
    /// The current motion finished (tested by every state with `& 0x8000`).
    pub const MOTION_END: u16 = 0x8000;
    /// fn_72_FE9C 0xFFB4: set on the opponent when an attack connects.
    pub const TAKE_HIT: u16 = 0x4000;
}

/// Fighter states (task+0x34 +0), fn_72_10E0C jump table (.data 0x17348).
pub mod state {
    pub const START: u8 = 0;
    pub const ENTRANCE: u8 = 1; // fn_72_F8F8
    pub const READY: u8 = 2; // fn_72_FAB8: charge until an attack
    pub const CHOOSE: u8 = 3; // fn_72_FCC8: pick the attack motion
    pub const ATTACK: u8 = 4; // fn_72_FE9C
    pub const SULK: u8 = 5; // fn_72_106F0
    pub const HURT: u8 = 6; // fn_72_1010C
    pub const STAGGER: u8 = 7; // fn_72_10AD4
    pub const RESULT: u8 = 11; // fn_72_10C98
}

/// Fighter parameters (work +0x20..+0x48) as fn_72_11038 sets them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fighter {
    /// +0x20 = 1000 + swim: subtracted from every incoming hit.
    pub defence: f32,
    /// +0x24 = 1000 + fly: lowers the chance to be hit.
    pub evasion: f32,
    /// +0x28 = (1000 + 1.5 run) / 1000: charge and knock-back rate.
    pub speed: f32,
    /// +0x2C = 1200 + power: attack strength.
    pub power: f32,
    /// +0x30 = 1000 + run / 2 - power / 3: accuracy.
    pub accuracy: f32,
    /// +0x34 = intellect: slows the zeal drain.
    pub intellect: f32,
    /// +0x38: zeal, 1000 at the start.
    pub zeal: f32,
    /// +0x3C: hit points; +0x44 the maximum, both 1000 + stamina.
    pub hp: f32,
    /// +0x40 = guts (skill 5), unjittered.
    pub luck: f32,
    pub hp_max: f32,
    /// +0x48: attack charge.
    pub charge: f32,
    /// work +4.
    pub flags: u16,
}

/// fn_72_11038 0x112DC..0x11A3C. `skills` are AL_ParameterGetSkill(0..=6)
/// (raw in the karate stage, CM 0x20934). Table opponents (`cpu`) get each of
/// swim, fly, run, power, stamina and intellect scaled by 1.05 - 0.1 * rand,
/// truncated; guts is used as is. `child` = Chao param +0x80 == 2, `d6` = +0xD6.
pub fn fighter(skills: [u16; 7], cpu: bool, child: bool, d6: bool, rng: &mut impl Rand) -> Fighter {
    let jitter = |s: u16, rng: &mut dyn FnMut() -> i32| -> u16 {
        if !cpu {
            return s;
        }
        // 0x11308..0x113A0: s = (int)(s * (1 + (0.05 - 0.1 * u))).
        let f2 = 1.0f32 + (0.05f32 - 0.1f32 * unit(rng()));
        (s as f32 * f2) as i32 as u16
    };
    let mut next = || rng.rand();
    let swim = jitter(skills[0], &mut next);
    let fly = jitter(skills[1], &mut next);
    let run = jitter(skills[2], &mut next);
    let power = jitter(skills[3], &mut next);
    let stamina = jitter(skills[4], &mut next);
    let intellect = jitter(skills[6], &mut next);
    let luck = skills[5];
    let hp = 1000.0f32 + stamina as f32 * 1.0f32;
    let mut flags = if cpu { flag::CPU } else { 0 };
    // 0x119F4: a child Chao (type 2) with run below SkillWalk.
    if child && run < native::SKILL_WALK {
        flags |= flag::SLOW_CHILD;
    }
    if d6 {
        flags |= flag::D6;
    }
    Fighter {
        defence: 1000.0f32 + swim as f32 * 1.0f32,
        evasion: 1000.0f32 + fly as f32 * 1.0f32,
        speed: (1000.0f32 + run as f32 * 1.5f32) / 1000.0f32,
        power: 1200.0f32 + power as f32 * 1.0f32,
        accuracy: (1000.0f32 + run as f32 / 2.0f32) - power as f32 / 3.0f32,
        intellect: intellect as f32 * 1.0f32,
        zeal: 1000.0,
        hp,
        luck: luck as f32,
        hp_max: hp,
        charge: 0.0,
        flags,
    }
}

/// fn_72_11038 0x11AB8: the entrance motion by guts (<1000, <2000, else).
pub fn entrance_motion(luck: f32) -> u8 {
    if luck < 1000.0 {
        0
    } else if luck < 2000.0 {
        1
    } else {
        2
    }
}

/// The state record (task+0x34) fields the rules read and write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateRecord {
    pub state: u8,
    pub step: u8,
    /// +3: frame counter, 0..59 (fn_72_10E0C 0x10F60).
    pub frame: u8,
    pub flags: u16,
    pub timer: u16,
}

/// What a take-hit step produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hurt {
    pub damage: f32,
    /// Reaction motion passed to fn_72_B620.
    pub motion: u8,
    /// The match manager's +3 was set to 2 (knock-out during a fight).
    pub knocked_out: bool,
}

/// fn_72_1010C, step 0 (0x10174..0x10524) falling into step 1. `attack` is the
/// attacker's +0x2C (fn_72_EDA4), `kind` the defender's work +6,
/// `fighting` = match manager +4 == 2.
pub fn take_hit(
    me: &mut Fighter,
    rec: &mut StateRecord,
    attack: f32,
    kind: i16,
    fighting: bool,
    rng: &mut impl Rand,
) -> Hurt {
    // 0x10174 clears 0x2000 (rlwinm mask 0x13..0x11).
    rec.flags &= !0x2000;
    let mut f30 = attack;
    // 0x1018C: one hit in ten rolls again for a 1.3x blow (u2 > 0.9).
    let critical = unit(rng.rand()) < 0.1f32 && unit(rng.rand()) > 0.9f32;
    if critical {
        f30 *= 1.3f32;
    } else {
        f30 *= 1.1f32 - 0.2f32 * unit(rng.rand());
    }
    let mut damage = f30 - me.defence;
    // 0x1029C: at least 5% of the maximum HP, except against kind 6.
    let floor = if kind == 6 {
        0.0f32
    } else {
        me.hp_max * (50.0f32 / 1000.0f32)
    };
    if damage < floor {
        damage = floor;
    }
    me.hp -= damage;
    let mut knocked_out = false;
    if me.hp < 0.0 {
        me.hp = 0.0;
        if fighting {
            knocked_out = true;
            rec.flags |= state_flag::KNOCKED_OUT;
        }
    }
    me.zeal -= damage / 3.0f32;
    if me.zeal < 0.0 {
        me.zeal = 0.0;
    }
    let heavy = rec.flags & state_flag::GIVE_UP != 0 || me.flags & 2 != 0;
    let motion = match (damage < 300.0, damage < 700.0, heavy) {
        (true, _, true) => 0x1D,
        (true, _, false) => 9,
        (false, true, true) => 0x1E,
        (false, true, false) => 0xA,
        (false, false, true) => 0x1F,
        (false, false, false) => 0xB,
    };
    me.flags &= !2;
    rec.flags &= !state_flag::GIVE_UP;
    rec.step = 1;
    hurt_step(me, rec, rng);
    Hurt {
        damage,
        motion,
        knocked_out,
    }
}

/// fn_72_1010C step 1 (0x10528..0x1069C), once per frame while hurt: the
/// knock-back adds to the charge; when the motion ends the fighter readies
/// (state 2), counters (state 3) or, at zero zeal with power >= 1600, staggers.
pub fn hurt_step(me: &mut Fighter, rec: &mut StateRecord, rng: &mut impl Rand) {
    let f1 = (1.0f32 + 0.1f32 * unit(rng.rand())) - 0.2f32;
    me.charge += 0.5f32 * (me.speed * f1);
    if rec.flags & state_flag::MOTION_END == 0 {
        return;
    }
    rec.step = 0;
    rec.state = if me.power < 1600.0 {
        if me.charge >= 400.0 {
            state::CHOOSE
        } else {
            state::READY
        }
    } else if me.zeal == 0.0 {
        state::STAGGER
    } else if me.charge >= 400.0f32 * 0.87f32 {
        state::CHOOSE
    } else {
        state::READY
    };
}

/// fn_72_EA20: does an attack with `accuracy` (the attacker's +0x30) reach this
/// defender? `distance` is fn_72_13258 (beyond 5.0 nothing happens).
pub fn hit_check(
    rec: &mut StateRecord,
    evasion: f32,
    kind: i16,
    accuracy: f32,
    distance: f32,
    rng: &mut impl Rand,
) -> bool {
    if distance > 5.0 {
        return false;
    }
    rec.flags |= state_flag::CHECKED;
    let mut p = if rec.state == state::STAGGER || rec.state == state::ATTACK {
        1.0f32
    } else if kind == 5 {
        0.5f32 * 0.1f32 + 0.6f32 * (accuracy / (accuracy + evasion))
    } else {
        0.1f32 + 1.5f32 * (accuracy / (accuracy + evasion))
    };
    if p > 1.0 {
        p = 1.0;
    }
    if unit(rng.rand()) <= p {
        rec.flags |= state_flag::HIT;
        true
    } else {
        rec.flags &= !state_flag::HIT;
        false
    }
}

/// fn_72_EDE8 for every state but 7 (the cheering branch is not ported): the
/// per-frame zeal drain and its clamp; in state 2 only, once a second below 500
/// zeal the give-up roll, and a set give-up flag moves the fighter to state 7.
/// `side` is work +2; `leader` is fn_72_133CC.
pub fn zeal_step(
    me: &mut Fighter,
    rec: &mut StateRecord,
    side: i16,
    leader: i16,
    rng: &mut impl Rand,
) {
    if matches!(rec.state, 1 | 0xA | 0xB) {
        return;
    }
    me.zeal -= 1000.0f32 / (1800.0f32 + 1800.0f32 * (me.intellect / 1000.0f32));
    if me.zeal > 1000.0 {
        me.zeal = 1000.0;
    } else if me.zeal < 0.0 {
        me.zeal = 0.0;
        rec.flags |= state_flag::GIVE_UP;
    }
    // 0xF37C: the rest only runs while ready (state 2).
    if rec.state != state::READY {
        return;
    }
    if rec.frame == 0 && me.zeal < 500.0 {
        let span = if side == leader { 2000.0f32 } else { 4000.0f32 };
        let keep = 1.0f32 - (500.0f32 - me.zeal) / span;
        if unit(rng.rand()) > keep {
            rec.flags |= state_flag::GIVE_UP;
        }
    }
    if rec.flags & state_flag::GIVE_UP != 0 {
        rec.state = state::STAGGER;
        rec.step = 0;
    }
}

/// fn_72_FAB8 step 1 (0xFBA0..0xFCB0), once per frame while ready.
pub fn ready_step(me: &mut Fighter, rec: &mut StateRecord, rng: &mut impl Rand) {
    let f2 = (1.0f32 + 0.15f32 * unit(rng.rand())) - 0.3f32;
    me.charge += me.speed * 1.0f32 * f2;
    let threshold = if me.power < 1600.0 {
        400.0f32
    } else {
        400.0f32 * 0.87f32
    };
    if me.charge >= threshold {
        rec.state = state::CHOOSE;
        rec.step = 0;
    }
}

/// fn_72_FE9C step 3 (0xFFD0..0x100EC): when the attack motion ends, a hit
/// gains 50 zeal (cap 1000), a miss costs 100 + 50 * guts / 1000.
pub fn attack_end(me: &mut Fighter, rec: &mut StateRecord) {
    if rec.flags & state_flag::MOTION_END == 0 {
        return;
    }
    rec.timer = 0x1E;
    if rec.flags & state_flag::ATTACK_HIT != 0 {
        me.zeal += 50.0;
        if me.zeal > 1000.0 {
            me.zeal = 1000.0;
        }
        rec.flags &= !state_flag::ATTACK_HIT;
    } else {
        let mut cost = 100.0f32 - -50.0f32 * (me.luck / 1000.0f32);
        if cost < 0.0 {
            cost = 0.0;
        }
        me.zeal -= cost;
        if me.zeal < 0.0 {
            me.zeal = 0.0;
        }
    }
    me.charge = 0.0;
    rec.state = state::READY;
    rec.step = 0;
}

/// fn_72_133CC: 0 or 1 for the fighter with the larger HP percentage, 2 when
/// both truncate (njRoundOff, main 0x80112320) to the same whole percent.
pub fn leader(hp: [(f32, f32); 2]) -> i16 {
    let pct = |(h, m): (f32, f32)| 100.0f32 * (h / m);
    let (a, b) = (pct(hp[0]), pct(hp[1]));
    if a.trunc() == b.trunc() {
        2
    } else if a > b {
        0
    } else {
        1
    }
}

/// fn_72_F4CC: round result. A knock-out (reason 3) is won by the fighter that
/// was not knocked out; at the time limit (reason 4) the HP leader wins, 2 = draw.
pub fn round_winner(knock_out: bool, player_knocked_out: bool, hp: [(f32, f32); 2]) -> i16 {
    if knock_out {
        i16::from(player_knocked_out)
    } else {
        leader(hp)
    }
}

/// fn_72_5020 0x50B0..0x514C: the opponent descriptor row for a class and
/// round; rows with an alternate (+8 >= 0) switch to it when rand / 32768 > 0.5.
pub fn opponent_row(
    rows: &[Descriptor],
    class: u8,
    round: u8,
    rng: &mut impl Rand,
) -> Option<usize> {
    let row = native::FIRST_OPPONENT_ROW + usize::from(class) * 5 + usize::from(round);
    let d = rows.get(row)?;
    if d.alternate >= 0 && unit(rng.rand()) > 0.5 {
        let alt = d.alternate as usize;
        return rows.get(alt).map(|_| alt);
    }
    Some(row)
}

/// One 10-byte opponent descriptor (ChaoStgKarate .data 0x1008; the same rows
/// are in sonic2app.exe at VA 0x01296940).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    /// +0: row in the skill table (.data 0x1118, 16 bytes per row).
    pub skills: u16,
    /// +2: name index (MsgChaoKarateName).
    pub name: u16,
    /// +4: appearance row (.data 0x12D8, 0x34 bytes per row).
    pub appearance: i8,
    /// +6: opponent kind (fn_72_1010C, fn_72_EA20, fn_72_F8F8).
    pub kind: i16,
    /// +8: alternate row, or -1.
    pub alternate: i8,
}

/// The opponent tables as the Steam executable carries them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tables {
    pub descriptors: Vec<Descriptor>,
    /// 8 u16 skills per row (swim, fly, run, power, stamina, guts, intellect, -).
    pub skills: Vec<[u16; 8]>,
}

/// sonic2app.exe VAs of the tables (equal to ChaoStgKarate .data 0x1008 / 0x1118).
pub const EXE_DESCRIPTORS: u32 = 0x0129_6940;
pub const EXE_SKILLS: u32 = 0x0129_6A50;
/// Row counts: .data 0x1008 size 0x110 (27 rows of 10), 0x1118 size 0x1C0 (28 of 16).
pub const DESCRIPTOR_ROWS: usize = 27;
pub const SKILL_ROWS: usize = 28;

impl Tables {
    /// Reads the tables from the user's own sonic2app.exe (little-endian data).
    pub fn read_exe(exe: &[u8]) -> crate::Result<Self> {
        let image = crate::pe::read_pe(exe)?;
        let at = image.va_to_offset(EXE_DESCRIPTORS, DESCRIPTOR_ROWS * 10)?;
        let descriptors = (0..DESCRIPTOR_ROWS)
            .map(|i| {
                let b = &exe[at + 10 * i..at + 10 * i + 10];
                Descriptor {
                    skills: u16::from_le_bytes([b[0], b[1]]),
                    name: u16::from_le_bytes([b[2], b[3]]),
                    appearance: b[4] as i8,
                    kind: i16::from_le_bytes([b[6], b[7]]),
                    alternate: b[8] as i8,
                }
            })
            .collect();
        let at = image.va_to_offset(EXE_SKILLS, SKILL_ROWS * 16)?;
        let skills = (0..SKILL_ROWS)
            .map(|i| {
                std::array::from_fn(|k| {
                    u16::from_le_bytes([exe[at + 16 * i + 2 * k], exe[at + 16 * i + 2 * k + 1]])
                })
            })
            .collect();
        Ok(Self {
            descriptors,
            skills,
        })
    }

    /// Reads the same tables from a GameCube ChaoStgKarate.rel (big-endian .data).
    pub fn read_rel(rel: &[u8]) -> crate::Result<Self> {
        let word = |o: usize| -> crate::Result<u32> {
            rel.get(o..o + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or_else(|| crate::Error::invalid("REL is truncated"))
        };
        if word(0)? != 72 {
            return Err(crate::Error::invalid(
                "not the GameCube ChaoStgKarate.rel (module 72)",
            ));
        }
        let data = (word(word(16)? as usize + 8 * 5)? & !1) as usize;
        let get = |o: usize, n: usize| {
            rel.get(data + o..data + o + n)
                .ok_or_else(|| crate::Error::invalid("REL is truncated"))
        };
        let d = get(0x1008, DESCRIPTOR_ROWS * 10)?;
        let descriptors = d
            .chunks(10)
            .map(|b| Descriptor {
                skills: u16::from_be_bytes([b[0], b[1]]),
                name: u16::from_be_bytes([b[2], b[3]]),
                appearance: b[4] as i8,
                kind: i16::from_be_bytes([b[6], b[7]]),
                alternate: b[8] as i8,
            })
            .collect();
        let s = get(0x1118, SKILL_ROWS * 16)?;
        let skills = s
            .chunks(16)
            .map(|b| std::array::from_fn(|k| u16::from_be_bytes([b[2 * k], b[2 * k + 1]])))
            .collect();
        Ok(Self {
            descriptors,
            skills,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msl_rand_sequence_starts_from_one() {
        let mut r = MslRand::default();
        // next = 1 * 0x41C64E6D + 0x3039 = 0x41C67EA6 -> 0x41C6 & 0x7FFF.
        assert_eq!(r.rand(), 0x41C6 & 0x7FFF);
    }

    #[test]
    fn a_player_fighter_uses_skills_unjittered() {
        let mut never = || -> i32 { panic!("no rand for the player's Chao") };
        let f = fighter(
            [100, 200, 300, 400, 500, 600, 700],
            false,
            false,
            false,
            &mut never,
        );
        assert_eq!(
            (f.defence, f.evasion, f.power, f.hp, f.hp_max, f.luck),
            (1100.0, 1200.0, 1600.0, 1500.0, 1500.0, 600.0)
        );
        assert_eq!(f.speed, (1000.0f32 + 450.0) / 1000.0);
        assert_eq!(f.zeal, 1000.0);
    }

    #[test]
    fn a_row_with_an_alternate_switches_above_one_half() {
        let row = |alternate| Descriptor {
            skills: 0,
            name: 0,
            appearance: 0,
            kind: -1,
            alternate,
        };
        let mut rows = vec![row(-1); 27];
        rows[5].alternate = 22;
        assert_eq!(opponent_row(&rows, 0, 3, &mut || 16384), Some(5));
        assert_eq!(opponent_row(&rows, 0, 3, &mut || 16385), Some(22));
        assert_eq!(opponent_row(&rows, 0, 0, &mut || 32767), Some(2));
        assert_eq!(opponent_row(&rows, 5, 0, &mut || 0), None);
    }
}
