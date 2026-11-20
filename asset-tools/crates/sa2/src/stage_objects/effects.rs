//! The shared particle effects of the stage modules: the creators that
//! EFFOBJ0's spawner table, ITEMBOX, ITEMBOXAIR and the ring pickup call,
//! and the per-frame exec callbacks their particle descriptors name.
//!
//! stg42D.rel section 1, creator / exec pairs (the exec is the function
//! pointer at descriptor +0x20, stg42D section 5):
//!
//! | creator | descriptor (s5) | exec |
//! |---------|-----------------|------|
//! | 0xA67C  | +0xDA40 | 0xA7F0 |
//! | 0xC238  | +0xDD18 | 0xC3D4 |
//! | 0xC56C `MakeParticle` | +0xDD50 | 0xC708 |
//! | 0xC8A0  | +0xDE70 | 0xCA94 |
//! | 0xCCFC  | +0xDFB0 | 0xCF04 (with the helper 0xCEE0) |
//! | 0xD0C4  | +0xDFE8 | 0xD2B8 (with 0xCEE0) |
//! | 0xD540  | +0xE020 | 0xD448 |
//! | 0xD714  | +0xE128 | 0xD8A0 |
//! | 0xE178  | +0xE1F4 | main.dol 0x80032A8C (engine, not ported) |
//! | 0xE334  | +0xE254 | 0xE4F8 |
//! | 0x1DFE0 | +0x1EEE8 | 0x1E6BC |
//! | 0x1E194 | +0x1EF20 | 0x1E6BC |
//!
//! 0x1DFE0 and 0x1E194 are the trail effects (Omochao's exec calls
//! 0x1DFE0); they are the same code with different descriptors, and
//! 0x1E6BC's code is 0xCA94's. `crate::duel_debris` has the same logic
//! (two draws for the angles, one added to the frame) inlined in its
//! debris update, written against the Steam gd_PC build; the ports here
//! are against the GameCube disc REL and GSNE8P main.dol (rand 0x8010DC88)
//! and do not share its addresses.
//!
//! Callers in stg42D: EFFOBJ0's exec 0x10C88 (stage-specific spawner
//! table; EFFOBJ0 itself belongs to `effect_emitter` and is not ported
//! here) calls 0xA67C, 0xC238, 0xC8A0, 0xCCFC, 0xD0C4, 0xD540, 0xD714, 0xE178,
//! 0xE334 (and the 0xD970 manager); ITEMBOX (0x123FC) and ITEMBOXAIR
//! (0x1337C) call 0xCCFC; `o_ring_0` (0x196B0) calls 0xC56C.
//!
//! Every creator has the same shape: `fn_80032B78(descriptor)` (main.dol,
//! the particle allocator: it takes a 0x40-byte particle from the free list
//! or `syCalloc`, fills it from the descriptor's template or the default at
//! 0x8019650C, links it to the descriptor and sets +0x18 from descriptor
//! +8); when that gives null nothing else happens. Otherwise the position
//! (+8), velocity (+0x20) and size (+4) are copied in and a fixed list of
//! fields is drawn from MSL `rand` (0x8010DC88), each as
//! `x = 2^-15 * rand()`, optionally `k * x`, optionally `x - off`, and for
//! angles `fctiwz(BAMS * x)` (BAMS = 182.04445, 65536/360); see [`draws`].
//! The constants are stg42D section-4 values, each named with its offset.
//!
//! The exec callbacks (`exec(descriptor, particle)`, nonzero return keeps
//! the particle) integrate the particle with the descriptor's frame step
//! (+0x10), drag (+0x14), gravity (+0x18) and size growth (+0x1C), then
//! compare the animation frame with the descriptor's frame count (+0xC);
//! see [`exec`].
//!
//! The spark manager 0xD970 / 0xDCC8 / 0xE0F8 ("SpSparkExec", also called
//! from EFFOBJ0's spawner table) is in [`sparks`].
pub mod sparks;

use crate::ninja_trig::{fctiwz, nj_sin};
use crate::prison_visuals::MslRand;

pub type V3 = [f32; 3];

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    r(
        "particle creator 0xA67C",
        0xa67c,
        0x174,
        "9f13765c1a6f6c64789609fd9b17df55278cace3ec6be04dc82fdea64deafc25",
    ),
    r(
        "particle exec 0xA7F0",
        0xa7f0,
        0x198,
        "9fd6ff66108eb265b3c2381ed3e4c2338c99f71a1bc586bd6093549250f78648",
    ),
    r(
        "particle creator 0xC238",
        0xc238,
        0x19c,
        "f726e4638d4a1305e9c1c3832214d9fe9ae54f9f3960d78c72e304f1665cd9e1",
    ),
    r(
        "particle exec 0xC3D4",
        0xc3d4,
        0x198,
        "9fd6ff66108eb265b3c2381ed3e4c2338c99f71a1bc586bd6093549250f78648",
    ),
    r(
        "MakeParticle",
        0xc56c,
        0x19c,
        "f726e4638d4a1305e9c1c3832214d9fe9ae54f9f3960d78c72e304f1665cd9e1",
    ),
    r(
        "particle exec 0xC708",
        0xc708,
        0x198,
        "9fd6ff66108eb265b3c2381ed3e4c2338c99f71a1bc586bd6093549250f78648",
    ),
    r(
        "particle creator 0xC8A0",
        0xc8a0,
        0x1f4,
        "bd825ddad6a4d32038df1ea9146a67f56d3386ebfe2da7f1b3e2677c2de8e1f0",
    ),
    r(
        "particle exec 0xCA94",
        0xca94,
        0xf8,
        "877c2f9c074fe809f5a7c6a8efa3e737b18dde8c74fd36319ff7e3d48992eb18",
    ),
    r(
        "particle creator 0xCCFC",
        0xccfc,
        0x1e4,
        "f41037c04a0953cd03e71c7af4b28ddb93e8c12117bc7e5018c0dbe72bc7dbee",
    ),
    r(
        "triangle wave 0xCEE0",
        0xcee0,
        0x24,
        "baa302653922b4c384cef519e6172482dee47e4f607691b464b935d6bc52dac6",
    ),
    r(
        "particle exec 0xCF04",
        0xcf04,
        0x1c0,
        "da848028a32c3cdde1a85e9d646cc66c8443617eaf1c02a60ff8b0ff5e08043c",
    ),
    r(
        "particle creator 0xD0C4",
        0xd0c4,
        0x1f4,
        "a39a5f6c8cfb10945322023d4d475f3dac4a5177a08aab9c2510ffbd217b32f7",
    ),
    r(
        "particle exec 0xD2B8",
        0xd2b8,
        0x190,
        "cd6eab4d8769ea618b2b8577c77ac248964a4d39df6afdf58f64f9e4bb2fa2b4",
    ),
    r(
        "particle exec 0xD448",
        0xd448,
        0xf8,
        "877c2f9c074fe809f5a7c6a8efa3e737b18dde8c74fd36319ff7e3d48992eb18",
    ),
    r(
        "particle creator 0xD540",
        0xd540,
        0x1d4,
        "574701a84db13c201922cb5a143949605afeed2cf4c6848e4f29eef3e882fc9b",
    ),
    r(
        "particle creator 0xD714",
        0xd714,
        0x18c,
        "3c6865ce28b15fd5e5d8459430f8830ded5ceb9c113c02602ed9c7856349dc81",
    ),
    r(
        "SpSparkExec spawner 0xD970",
        0xd970,
        0x358,
        "6864cf41373fbc74e4b4f1c9013d14b1098e61e211e360e9ce811e84dfe47956",
    ),
    // 0xDCC8 without its two unrelocated `bl fabsf` (0xDDC0, 0xDE4C).
    r(
        "SpSparkExec exec 0xDCC8 (1/3)",
        0xdcc8,
        0xf8,
        "5fcb25e9a175cb26527f03b5d8164a872ae9b5fb87ce9b3a1ba89f42abca27e7",
    ),
    r(
        "SpSparkExec exec 0xDCC8 (2/3)",
        0xddc4,
        0x88,
        "974d32d893fee1f6abb25567f11975c809e350b2c7cfed39b25c5795c2204b99",
    ),
    r(
        "SpSparkExec exec 0xDCC8 (3/3)",
        0xde50,
        0x138,
        "6a702b17c9f748f5528430c08f3a10cf4c0ab6f11961bac67974a373c0caadc5",
    ),
    r(
        "SpSparkExec destructor 0xE0F8",
        0xe0f8,
        0x80,
        "8df4c73651959bfdd4a270d017c9962b045b71cf9722226b8b69f2f37591ec03",
    ),
    r(
        "particle exec 0xD8A0",
        0xd8a0,
        0xd0,
        "58aa7f26228e96a98df978bbcc4f5112eb5993f420d38dc3e68cfb4d1c678464",
    ),
    r(
        "particle creator 0xE178",
        0xe178,
        0x1bc,
        "7ddf90fc116b44cc0eafc86cc23e472fa40f4a1478bab748ac0cf7e5674016ed",
    ),
    r(
        "particle creator 0xE334",
        0xe334,
        0x1c4,
        "401331ad87b30327daedef1d79a37b3ece70d4436e6d2d37bf87910456413432",
    ),
    r(
        "trail effect 0x1DFE0",
        0x1dfe0,
        0x1b4,
        "ac7e00fd2f44d9b07d5fd57b1d5a2245589ac0559d29a1dd4c6511940a9dd368",
    ),
    r(
        "trail effect 0x1E194",
        0x1e194,
        0x1b4,
        "ac7e00fd2f44d9b07d5fd57b1d5a2245589ac0559d29a1dd4c6511940a9dd368",
    ),
    r(
        "particle exec 0x1E6BC",
        0x1e6bc,
        0xf8,
        "877c2f9c074fe809f5a7c6a8efa3e737b18dde8c74fd36319ff7e3d48992eb18",
    ),
    r(
        "particle exec 0xE4F8",
        0xe4f8,
        0x1ec,
        "45381e25fb92e02708e7b1d6d36a5c9481c7d1246f805a98db6800808a60d745",
    ),
];

const fn r(name: &'static str, stg42d: u32, len: u32, h: &'static str) -> super::Routine {
    super::Routine {
        name,
        stg42d,
        len,
        masked_sha256: h,
    }
}

/// The 0x40-byte particle work the allocator hands out and the exec
/// callbacks update.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Particle {
    /// +0x00: first angle (BAMS), advanced by `spin` each frame.
    pub angle: i32,
    /// +0x04: size.
    pub size: f32,
    /// +0x08: position.
    pub pos: V3,
    /// +0x14: colour, alpha in the top byte.
    pub color: u32,
    /// +0x18: animation frame.
    pub frame: f32,
    /// +0x1C: second angle (BAMS).
    pub angle2: i32,
    /// +0x20: velocity.
    pub vel: V3,
    /// +0x2C: spin added to `angle` each frame.
    pub spin: i32,
    /// +0x30: next particle of the descriptor (the allocator's).
    pub link: u32,
    /// +0x34: flags (the allocator's).
    pub flags: u32,
    /// +0x38: the effect's own word: an int phase or a float, by effect.
    pub work: u32,
    /// +0x3C.
    pub tail: u32,
}

impl Particle {
    /// The particle as 16 big-endian words (its memory image).
    pub fn to_words(&self) -> [u32; 16] {
        let f = f32::to_bits;
        [
            self.angle as u32,
            f(self.size),
            f(self.pos[0]),
            f(self.pos[1]),
            f(self.pos[2]),
            self.color,
            f(self.frame),
            self.angle2 as u32,
            f(self.vel[0]),
            f(self.vel[1]),
            f(self.vel[2]),
            self.spin as u32,
            self.link,
            self.flags,
            self.work,
            self.tail,
        ]
    }

    /// The particle from its memory image.
    pub fn from_words(w: [u32; 16]) -> Self {
        let f = f32::from_bits;
        Self {
            angle: w[0] as i32,
            size: f(w[1]),
            pos: [f(w[2]), f(w[3]), f(w[4])],
            color: w[5],
            frame: f(w[6]),
            angle2: w[7] as i32,
            vel: [f(w[8]), f(w[9]), f(w[10])],
            spin: w[11] as i32,
            link: w[12],
            flags: w[13],
            work: w[14],
            tail: w[15],
        }
    }
}

/// The descriptor fields the exec callbacks read (stage REL section 5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Descriptor {
    /// +0x0C: frame count (compared unsigned).
    pub frames: u32,
    /// +0x10: added to the frame each update.
    pub frame_step: f32,
    /// +0x14: velocity factor each update.
    pub drag: f32,
    /// +0x18: added to the y velocity after the drag.
    pub gravity: f32,
    /// +0x1C: added to the size each update.
    pub growth: f32,
}

/// What the creators ask of the engine.
pub trait EffectHost {
    /// fn_80032B78(descriptor) (main.dol): allocate a particle for the
    /// descriptor at this section-5 offset of the stage REL, already filled
    /// as the allocator fills it; `None` when the allocator returns null.
    fn particle_alloc(&mut self, descriptor: u32) -> Option<Particle>;
}

/// BAMS per degree, the same single in every table (stg42D s4 +0x4B0,
/// +0x4E8, +0x510, +0x550, +0x5A0, +0x5D0).
pub const BAMS: f32 = f32::from_bits(0x4336_0b61);
/// 2^-15, the `rand` scale (s4 +0x4B8, +0x4F0, +0x518, +0x558, +0x5A8,
/// +0x5C8).
pub const RAND_SCALE: f32 = f32::from_bits(0x3800_0000);

/// Where a drawn value goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    /// +0x00 (`fctiwz(BAMS * x)`).
    Angle,
    /// +0x1C (`fctiwz(BAMS * x) + 0x4000`).
    Angle2,
    /// +0x2C (`fctiwz(BAMS * x)`).
    Spin,
    /// +0x38 as a float.
    Work,
    /// +0x18 replaced.
    FrameSet,
    /// +0x18 increased (after the allocator set it).
    FrameAdd,
}

/// One `rand` draw: `x = RAND_SCALE * rand()`, then `k * x`, then `x - off`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw {
    pub k: Option<f32>,
    pub off: Option<f32>,
    pub dest: Dest,
}

const fn d(k: Option<f32>, off: Option<f32>, dest: Dest) -> Draw {
    Draw { k, off, dest }
}
const fn c(bits: u32) -> Option<f32> {
    Some(f32::from_bits(bits))
}

/// A field a creator sets without drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fixed {
    /// +0x38 = 0 (int).
    WorkZero,
    /// +0x38 = the creator's second float argument.
    WorkArg,
    /// +0x18 = this constant.
    Frame(f32),
    /// +0x14 = this colour word.
    Color(u32),
}

/// The particle creators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Creator {
    A67c,
    C238,
    MakeParticle,
    C8a0,
    Ccfc,
    D0c4,
    D540,
    D714,
    E178,
    E334,
    /// 0x1DFE0, the trail effect.
    X1dfe0,
    /// 0x1E194, its twin.
    X1e194,
}

const F80: u32 = 0x42a0_0000;
const F40: u32 = 0x4220_0000;
const F16: u32 = 0x4180_0000;
const HALF: u32 = 0x3f00_0000;

/// Each creator's draws, in `rand` order, and the fields it sets without
/// drawing, with their place among the draws (the index of the draw they
/// come before); values from stg42D section 4.
pub fn draws(kind: Creator) -> (&'static [Draw], &'static [(usize, Fixed)]) {
    use Dest::*;
    match kind {
        // 0xA67C: 20 (+0x40C), 10 (+0x414), 0.5 (+0x418).
        Creator::A67c => {
            const D: &[Draw] = &[
                d(c(0x41a0_0000), c(0x4120_0000), Angle),
                d(None, c(HALF), Spin),
            ];
            const F: &[(usize, Fixed)] = &[(2, Fixed::WorkZero)];
            (D, F)
        }
        // 0xC238: 15 (+0x4B4), 7.5 (+0x4BC), 0.5 (+0x4C0).
        Creator::C238 => {
            const D: &[Draw] = &[
                d(c(0x4170_0000), c(0x40f0_0000), Angle),
                d(None, c(HALF), Spin),
                d(c(HALF), None, FrameSet),
            ];
            const F: &[(usize, Fixed)] = &[(2, Fixed::WorkZero)];
            (D, F)
        }
        // 0xC56C: 10 (+0x4D0), 5 (+0x4E0), 0.5 (+0x4C0).
        Creator::MakeParticle => {
            const D: &[Draw] = &[
                d(c(0x4120_0000), c(0x40a0_0000), Angle),
                d(None, c(HALF), Spin),
                d(c(HALF), None, FrameSet),
            ];
            const F: &[(usize, Fixed)] = &[(2, Fixed::WorkZero)];
            (D, F)
        }
        // 0xC8A0: 20 (+0x4EC), 10 (+0x4F4), 5 (+0x4F8), 0.5 (+0x4FC).
        Creator::C8a0 => {
            const D: &[Draw] = &[
                d(c(0x41a0_0000), c(0x4120_0000), Angle),
                d(c(0x4120_0000), c(0x40a0_0000), Angle2),
                d(None, c(HALF), Spin),
                d(None, None, FrameAdd),
            ];
            const F: &[(usize, Fixed)] = &[];
            (D, F)
        }
        // 0xCCFC: 80 (+0x514), 40 (+0x51C), 0.5 (+0x520), 16 (+0x524).
        Creator::Ccfc => {
            const D: &[Draw] = &[
                d(c(F80), c(F40), Angle),
                d(None, c(HALF), Spin),
                d(c(F16), None, Work),
                d(None, None, FrameAdd),
            ];
            const F: &[(usize, Fixed)] = &[];
            (D, F)
        }
        // 0xD0C4: 80, 40, 1.3 (+0x540), 0.6 (+0x544), 16 (+0x524).
        Creator::D0c4 => {
            const D: &[Draw] = &[
                d(c(F80), c(F40), Angle),
                d(c(0x3fa6_6666), c(0x3f19_999a), Spin),
                d(c(F16), None, Work),
                d(None, None, FrameAdd),
            ];
            const F: &[(usize, Fixed)] = &[];
            (D, F)
        }
        // 0xD540: 80, 40, 0.2 (+0x548), 0.1 (+0x54C); the colour is the
        // word at s5 +0xE058.
        Creator::D540 => {
            const D: &[Draw] = &[
                d(c(F80), c(F40), Angle),
                d(c(0x3e4c_cccd), c(0x3dcc_cccd), Spin),
                d(None, None, FrameAdd),
            ];
            const F: &[(usize, Fixed)] = &[(2, Fixed::WorkArg), (3, Fixed::Color(0xff80_ff50))];
            (D, F)
        }
        // 0xD714: 80 (+0x554), 40 (+0x55C), 0.5 (+0x560), 0.25 (+0x564),
        // frame 0.0 (+0x568).
        Creator::D714 => {
            const D: &[Draw] = &[d(c(F80), c(F40), Angle), d(c(HALF), c(0x3e80_0000), Spin)];
            const F: &[(usize, Fixed)] = &[(2, Fixed::Frame(0.0))];
            (D, F)
        }
        // 0xE178: 360 (+0x5A4), 180 (+0x5AC), 0.2 (+0x5B0), 0.8 (+0x5B4),
        // 0.4 (+0x5B8).
        Creator::E178 => {
            const D: &[Draw] = &[
                d(c(0x43b4_0000), c(0x4334_0000), Angle),
                d(c(0x3e4c_cccd), None, FrameSet),
                d(c(0x3f4c_cccd), c(0x3ecc_cccd), Spin),
            ];
            const F: &[(usize, Fixed)] = &[];
            (D, F)
        }
        // 0xE334: 360 (+0x5D4), 20 (+0x5E0), 3 (+0x5E4), 1.5 (+0x5E8).
        Creator::E334 => {
            const D: &[Draw] = &[
                d(c(0x43b4_0000), None, Angle),
                d(c(0x41a0_0000), None, Angle2),
                d(c(0x4040_0000), c(0x3fc0_0000), Spin),
            ];
            const F: &[(usize, Fixed)] = &[];
            (D, F)
        }
        // 0x1DFE0 / 0x1E194: 20 (+0x9DC), 10 (+0x9E4), 0.5 (+0x9E8); BAMS
        // (+0x9D8) and 2^-15 (+0x9E0) are the shared values.
        Creator::X1dfe0 | Creator::X1e194 => {
            const D: &[Draw] = &[
                d(c(0x41a0_0000), c(0x4120_0000), Angle),
                d(None, c(HALF), Spin),
                d(None, None, FrameAdd),
            ];
            const F: &[(usize, Fixed)] = &[(2, Fixed::WorkArg)];
            (D, F)
        }
    }
}

impl Creator {
    pub const ALL: [Creator; 12] = [
        Creator::A67c,
        Creator::C238,
        Creator::MakeParticle,
        Creator::C8a0,
        Creator::Ccfc,
        Creator::D0c4,
        Creator::D540,
        Creator::D714,
        Creator::E178,
        Creator::E334,
        Creator::X1dfe0,
        Creator::X1e194,
    ];

    /// The creator's offset in stg42D.rel section 1.
    pub fn stg42d(self) -> u32 {
        match self {
            Self::A67c => 0xa67c,
            Self::C238 => 0xc238,
            Self::MakeParticle => 0xc56c,
            Self::C8a0 => 0xc8a0,
            Self::Ccfc => 0xccfc,
            Self::D0c4 => 0xd0c4,
            Self::D540 => 0xd540,
            Self::D714 => 0xd714,
            Self::E178 => 0xe178,
            Self::E334 => 0xe334,
            Self::X1dfe0 => 0x1dfe0,
            Self::X1e194 => 0x1e194,
        }
    }

    /// Its descriptor in stg42D section 5 (each stage REL has its own copy
    /// at its own offset).
    pub fn stg42d_descriptor(self) -> u32 {
        match self {
            Self::A67c => 0xda40,
            Self::C238 => 0xdd18,
            Self::MakeParticle => 0xdd50,
            Self::C8a0 => 0xde70,
            Self::Ccfc => 0xdfb0,
            Self::D0c4 => 0xdfe8,
            Self::D540 => 0xe020,
            Self::D714 => 0xe128,
            Self::E178 => 0xe1f4,
            Self::E334 => 0xe254,
            Self::X1dfe0 => 0x1eee8,
            Self::X1e194 => 0x1ef20,
        }
    }
}

/// `2^-15 * rand()`: the int-to-float conversion through the 0x43300000
/// magic double (exact for `rand`'s range) and the scale.
fn unit(r: u32) -> f32 {
    RAND_SCALE * r as i32 as f32
}

/// The arguments a creator is called with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spawn {
    /// r3: the position.
    pub pos: V3,
    /// r4: the velocity.
    pub vel: V3,
    /// f1: the size.
    pub size: f32,
    /// f2: 0xD540's work value (ignored by the others).
    pub work: f32,
}

/// Run a creator: `descriptor` is its descriptor's section-5 offset in the
/// stage REL. Returns the particle as the creator leaves it, or `None` when
/// the allocator failed (then nothing is drawn).
pub fn create(
    kind: Creator,
    descriptor: u32,
    a: &Spawn,
    rand: &mut MslRand,
    host: &mut impl EffectHost,
) -> Option<Particle> {
    let mut p = host.particle_alloc(descriptor)?;
    p.pos = a.pos;
    p.vel = a.vel;
    p.size = a.size;
    let (draws, fixed) = draws(kind);
    let apply_fixed = |p: &mut Particle, at: usize| {
        for (_, f) in fixed.iter().filter(|(i, _)| *i == at) {
            match *f {
                Fixed::WorkZero => p.work = 0,
                Fixed::WorkArg => p.work = a.work.to_bits(),
                Fixed::Frame(v) => p.frame = v,
                Fixed::Color(v) => p.color = v,
            }
        }
    };
    for (i, dr) in draws.iter().enumerate() {
        apply_fixed(&mut p, i);
        let mut x = unit(rand.rand());
        if let Some(k) = dr.k {
            x *= k;
        }
        if let Some(off) = dr.off {
            x -= off;
        }
        let angle = || fctiwz(BAMS * x);
        match dr.dest {
            Dest::Angle => p.angle = angle(),
            Dest::Angle2 => p.angle2 = angle().wrapping_add(0x4000),
            Dest::Spin => p.spin = angle(),
            Dest::Work => p.work = x.to_bits(),
            Dest::FrameSet => p.frame = x,
            Dest::FrameAdd => p.frame += x,
        }
    }
    for at in draws.len()..=draws.len() + 1 {
        apply_fixed(&mut p, at);
    }
    Some(p)
}

/// The exec callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exec {
    /// 0xA7F0 (for 0xA67C).
    A7f0,
    /// 0xC3D4 (for 0xC238).
    C3d4,
    /// 0xC708 (for MakeParticle).
    C708,
    /// 0xCA94 (for 0xC8A0).
    Ca94,
    /// 0xCF04 (for 0xCCFC).
    Cf04,
    /// 0xD2B8 (for 0xD0C4).
    D2b8,
    /// 0xD448 (for 0xD540).
    D448,
    /// 0xD8A0 (for 0xD714).
    D8a0,
    /// 0xE4F8 (for 0xE334).
    E4f8,
    /// 0x1E6BC (for 0x1DFE0 and 0x1E194).
    X1e6bc,
}

impl Exec {
    pub const ALL: [Exec; 10] = [
        Exec::A7f0,
        Exec::C3d4,
        Exec::C708,
        Exec::Ca94,
        Exec::Cf04,
        Exec::D2b8,
        Exec::D448,
        Exec::D8a0,
        Exec::E4f8,
        Exec::X1e6bc,
    ];

    /// The exec's offset in stg42D.rel section 1.
    pub fn stg42d(self) -> u32 {
        match self {
            Self::A7f0 => 0xa7f0,
            Self::C3d4 => 0xc3d4,
            Self::C708 => 0xc708,
            Self::Ca94 => 0xca94,
            Self::Cf04 => 0xcf04,
            Self::D2b8 => 0xd2b8,
            Self::D448 => 0xd448,
            Self::D8a0 => 0xd8a0,
            Self::E4f8 => 0xe4f8,
            Self::X1e6bc => 0x1e6bc,
        }
    }
}

/// 0xCEE0: a triangle wave of period `2n` over an unsigned count
/// (`x mod 2n`, folded back from `n` up).
pub fn triangle(x: u32, n: u32) -> u32 {
    let m = n.wrapping_shl(1);
    // divwu by zero gives 0 in the harness (QEMU) and the routine then
    // keeps x; every caller passes 16.
    let r = x.checked_rem(m).unwrap_or(x);
    if r >= n {
        m.wrapping_sub(r)
    } else {
        r
    }
}

/// `__cvt_fp2unsigned` (main.dol 0x8010AF3C): 0 below zero, 0xFFFFFFFF
/// from 2^32 up and for NaN (its `bge` is "not less than", which an
/// unordered compare satisfies), otherwise truncation.
pub fn cvt_fp2unsigned(x: f64) -> u32 {
    if x < 0.0 {
        0
    } else if x.is_nan() || x >= 4_294_967_296.0 {
        u32::MAX
    } else {
        x as u32
    }
}

/// The common start of every exec but 0xE4F8: spin, growth, position,
/// drag and gravity, frame step.
fn integrate(desc: &Descriptor, p: &mut Particle) {
    p.angle = p.angle.wrapping_add(p.spin);
    p.size += desc.growth;
    for k in 0..3 {
        p.pos[k] += p.vel[k];
    }
    p.vel[0] *= desc.drag;
    p.vel[1] = desc.gravity + p.vel[1] * desc.drag;
    p.vel[2] *= desc.drag;
    p.frame += desc.frame_step;
}

/// Has the frame reached the frame count? (`fctiwz`, sign-extended from
/// 16 bits, compared unsigned.)
fn frame_done(desc: &Descriptor, p: &Particle) -> bool {
    (fctiwz(p.frame) as i16 as i32 as u32) >= desc.frames
}

/// Hold the last frame: `frames - 1` through the unsigned magic double,
/// rounded to single.
fn clamp_frame(desc: &Descriptor, p: &mut Particle) {
    p.frame = desc.frames.wrapping_sub(1) as f64 as f32;
}

/// Halve the alpha; alive while it is nonzero.
fn fade(p: &mut Particle) -> u32 {
    p.color = (p.color & 0x00ff_ffff) | ((p.color >> 1) & 0xff00_0000);
    u32::from(p.color & 0xff00_0000 != 0)
}

/// Run one exec frame on a particle; the return value is what the routine
/// returns in r3 (the engine keeps the particle while it is nonzero).
pub fn exec(kind: Exec, desc: &Descriptor, p: &mut Particle, rand: &mut MslRand) -> u32 {
    match kind {
        Exec::C3d4 | Exec::C708 | Exec::A7f0 => {
            integrate(desc, p);
            // njSin (0x80112398) of the phase, times 10 (s4 +0x4D0; +0x414
            // for 0xA7F0) and BAMS (+0x4B0; +0x408); the phase steps by 0x222.
            let s = nj_sin(p.work as i32);
            p.angle2 = fctiwz(BAMS * (f32::from_bits(0x4120_0000) * s)).wrapping_add(0x4000);
            p.work = p.work.wrapping_add(0x222);
            if frame_done(desc, p) {
                clamp_frame(desc, p);
                fade(p)
            } else {
                1
            }
        }
        Exec::Cf04 | Exec::D2b8 => {
            integrate(desc, p);
            // 0xCEE0(__cvt_fp2unsigned(work), 16) - 8, times BAMS (+0x510);
            // the work float steps by 0.26666668 (+0x530).
            let w = f32::from_bits(p.work);
            let t = triangle(cvt_fp2unsigned(f64::from(w)), 16).wrapping_sub(8) as i32;
            p.angle2 = fctiwz(BAMS * t as f32).wrapping_add(0x4000);
            p.work = (w + f32::from_bits(0x3e88_8889)).to_bits();
            if frame_done(desc, p) {
                clamp_frame(desc, p);
                if kind == Exec::Cf04 {
                    fade(p)
                } else {
                    0
                }
            } else {
                1
            }
        }
        Exec::Ca94 | Exec::D448 | Exec::X1e6bc => {
            integrate(desc, p);
            if frame_done(desc, p) {
                clamp_frame(desc, p);
                0
            } else {
                1
            }
        }
        Exec::D8a0 => {
            integrate(desc, p);
            u32::from(!frame_done(desc, p))
        }
        Exec::E4f8 => exec_e4f8(desc, p, rand),
    }
}

/// 0xE4F8: no frame step; a 3 % chance (`x < 0.03`, s4 +0x5EC) per frame
/// of a sideways kick `0.2 * (x - 0.5)` (+0x5F0, +0x5F4) to x then z, then
/// drag and gravity; the alpha drops by 2 while it is at least 10 and the
/// size is not below zero (+0x5CC; `bge` after `fcmpo`, so a NaN size
/// counts as not below), otherwise the particle ends.
/// When it lives the routine returns whatever `rand` returned last (r3 is
/// never set on that path).
fn exec_e4f8(desc: &Descriptor, p: &mut Particle, rand: &mut MslRand) -> u32 {
    p.angle = p.angle.wrapping_add(p.spin);
    p.size += desc.growth;
    for k in 0..3 {
        p.pos[k] += p.vel[k];
    }
    let mut last = rand.rand();
    if unit(last) < f32::from_bits(0x3cf5_c28f) {
        let kick = |r: u32| f32::from_bits(0x3e4c_cccd) * (unit(r) - 0.5);
        p.vel[0] += kick(rand.rand());
        last = rand.rand();
        p.vel[2] += kick(last);
    }
    p.vel[0] *= desc.drag;
    p.vel[1] = desc.gravity + p.vel[1] * desc.drag;
    p.vel[2] *= desc.drag;
    let alpha = p.color >> 24;
    if alpha < 10 || p.size < 0.0 {
        return 0;
    }
    p.color = (p.color & 0x00ff_ffff) | ((alpha - 2) << 24);
    last
}
