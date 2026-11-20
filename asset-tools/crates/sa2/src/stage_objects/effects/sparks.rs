//! The shared spark manager "SpSparkExec": a ring of up to 254 live sparks,
//! each a short chain of six points that falls, slows and bounces on a
//! floor height, kept by one task for the whole stage.
//!
//! stg42D.rel section 1: the spawner 0xD970 (called by EFFOBJ0's spawner
//! table 0x10C88 and by the stage's 0x98E4), the task's exec 0xDCC8 and its
//! destructor 0xE0F8. The display 0xDF88 only draws (njSetTexture and
//! the Ninja line calls) and is not ported. The manager's state is three
//! words of the stage REL's BSS (stg42D section 6 +0x660 buffer, +0x664
//! task, +0x668 reference count), here [`Sparks`].
//!
//! 0xDCC8 calls the module's local `fabsf` (stg42D 0x84, one `fabs`) with a
//! relative branch the REL does not relocate, so its bytes differ between
//! stage RELs only in those two branch displacements (0xDDC0, 0xDE4C); the
//! shared test masks the routine without them.
use crate::prison_visuals::MslRand;

pub type V3 = [f32; 3];

/// Ring size (the `% 255` the routines compute with the 0x80808081
/// reciprocal).
pub const SLOTS: i32 = 255;
/// A new spark lives this many exec frames (0xD970 stores 0x50).
pub const LIFE: i32 = 0x50;
/// The spawner's jitter scale, 2.0 (s4 +0x578), and the rand scale 2^-15
/// (+0x57C) with its 0.5 centre (+0x580).
const JITTER: f32 = 2.0;
const RAND_SCALE: f32 = f32::from_bits(0x3800_0000);
/// The exec's damping per point along the chain, 0.97 (s4 +0x590).
const CHAIN: f32 = f32::from_bits(0x3f78_51ec);
/// Velocity factor 0.98 (s5 +0xE17C), bounce 0.7 (+0xE180) and gravity
/// 0.08 (+0xE184): stage data, the same in every REL carrying the routine.
const DRAG: f32 = f32::from_bits(0x3f7a_e148);
const BOUNCE: f32 = f32::from_bits(0x3f33_3333);
const GRAVITY: f32 = f32::from_bits(0x3da3_d70a);

/// One spark (0x5C bytes).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spark {
    /// +0x00: the six points, the head first.
    pub points: [V3; 6],
    /// +0x48: the head's velocity.
    pub vel: V3,
    /// +0x54: the floor height.
    pub floor: f32,
    /// +0x58: frames left.
    pub life: i32,
}

/// The 0x5BAC-byte buffer `syCalloc` gives the manager.
#[derive(Debug, Clone, PartialEq)]
pub struct SparkBuffer {
    pub sparks: Vec<Spark>,
    /// +0x5BA4: the oldest live slot.
    pub head: i32,
    /// +0x5BA8: how many are live.
    pub count: i32,
}

impl Default for SparkBuffer {
    fn default() -> Self {
        Self {
            sparks: vec![Spark::default(); SLOTS as usize],
            head: 0,
            count: 0,
        }
    }
}

impl SparkBuffer {
    /// The buffer's memory image (big-endian, as the game holds it).
    pub fn to_be_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x5bac);
        for s in &self.sparks {
            for v in s.points.iter().chain([&s.vel]) {
                for x in v {
                    out.extend(x.to_bits().to_be_bytes());
                }
            }
            out.extend(s.floor.to_bits().to_be_bytes());
            out.extend(s.life.to_be_bytes());
        }
        out.extend(self.head.to_be_bytes());
        out.extend(self.count.to_be_bytes());
        out
    }
}

/// The manager's globals (stage REL section 6 +0x660..+0x668).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sparks {
    /// +0x660: the buffer, once allocated.
    pub buffer: Option<SparkBuffer>,
    /// +0x664: the manager task (0 when none).
    pub task: u32,
    /// +0x668: how many manager tasks were created and not yet destroyed.
    pub refs: i32,
}

/// What the manager asks of the engine.
pub trait SparkHost {
    /// CreateElementalTask(0, 2, exec 0xDCC8, "SpSparkExec") (0x80071E0C);
    /// the spawner then sets the task's exec (+0x10), display (+0x14,
    /// 0xDF88) and destructor (+0x18, 0xE0F8). Returns the task, 0 when
    /// none was made.
    fn create_task(&mut self) -> u32;
    /// syCalloc(1, 0x5BAC) (0x801197FC): false when it returns null.
    fn calloc_buffer(&mut self) -> bool;
    /// syFree(buffer) (0x80119828).
    fn free_buffer(&mut self);
    /// DestroyTask(task) (0x80071AF0); the engine later runs the task's
    /// destructor ([`Sparks::destroy`]).
    fn destroy_task(&mut self, task: u32);
}

fn unit(rand: &mut MslRand) -> f32 {
    RAND_SCALE * rand.rand() as i32 as f32
}

impl Sparks {
    /// 0xD970(pos, vel, size, floor): start the manager if needed and add
    /// a spark at `pos`, moving at `vel` plus a random jitter of up to
    /// `size` per axis; when the ring is full the oldest is replaced.
    pub fn spawn(
        &mut self,
        pos: V3,
        vel: V3,
        size: f32,
        floor: f32,
        rand: &mut MslRand,
        host: &mut impl SparkHost,
    ) {
        if self.task == 0 {
            let t = host.create_task();
            if t != 0 {
                self.task = t;
                self.refs = self.refs.wrapping_add(1);
            }
        }
        if self.task != 0 && self.buffer.is_none() && host.calloc_buffer() {
            self.buffer = Some(SparkBuffer::default());
        }
        let (true, Some(buf)) = (self.task != 0, self.buffer.as_mut()) else {
            if self.buffer.take().is_some() {
                host.free_buffer();
            }
            if self.task != 0 {
                host.destroy_task(self.task);
            }
            return;
        };
        let mut n = buf.count;
        if n < 0xfe {
            n = n.wrapping_add(1);
        } else {
            buf.head = buf.head.wrapping_add(1) % SLOTS;
        }
        buf.count = n;
        let mut slot = n.wrapping_add(buf.head).wrapping_sub(1);
        if slot < 0 {
            slot = 0xfe;
        }
        slot %= SLOTS;
        let span = size * JITTER;
        let jx = span * (unit(rand) - 0.5);
        let jy = span * (unit(rand) - 0.5);
        let jz = span * (unit(rand) - 0.5);
        let s = &mut buf.sparks[slot as usize];
        s.vel = [jx + vel[0], jy + vel[1], jz + vel[2]];
        s.points = [pos; 6];
        s.floor = floor;
        s.life = LIFE;
    }

    /// 0xDCC8, the manager task's exec.
    pub fn exec(&mut self, task: u32, host: &mut impl SparkHost) {
        let Some(buf) = self.buffer.as_mut() else {
            host.destroy_task(task);
            return;
        };
        let mut slot = buf.head;
        let mut last = -1;
        for _ in 0..buf.count {
            // A negative slot would read before the buffer; the routine
            // never makes one (head and count stay in range).
            let s = &mut buf.sparks[slot as usize];
            let [mut vx, vy0, mut vz] = s.vel;
            let [mut x, mut y, mut z] = s.points[0];
            vx *= DRAG;
            let mut vy = vy0 * DRAG - GRAVITY;
            vz *= DRAG;
            x += vx;
            y += vy;
            z += vz;
            if y < s.floor {
                y = s.floor;
                vy = BOUNCE * vy.abs();
            }
            s.vel = [vx, vy, vz];
            s.points[0] = [x, y, z];
            let k = CHAIN * DRAG;
            for p in &mut s.points[1..] {
                vx *= k;
                vy = CHAIN * (vy * DRAG) - GRAVITY;
                vz *= k;
                x += vx;
                y += vy;
                z += vz;
                if y < s.floor {
                    y = s.floor;
                    vy = BOUNCE * vy.abs();
                }
                *p = [x, y, z];
            }
            s.life = s.life.wrapping_sub(1);
            if s.life <= 0 {
                last = slot;
            }
            slot = slot.wrapping_add(1) % SLOTS;
        }
        // Drop every spark up to the last one that expired: the ring starts
        // after it, and the count shrinks by the slots skipped (+0xFF when
        // the new head wrapped past the old one).
        if last >= 0 {
            let old = buf.head;
            let end = old.wrapping_add(buf.count);
            let mut first = last.wrapping_add(1) % SLOTS;
            buf.head = first;
            if first < old {
                first = first.wrapping_add(0xff);
            }
            buf.count = end.wrapping_sub(first);
        }
        if buf.count == 0 {
            host.destroy_task(task);
        }
    }

    /// 0xE0F8, the manager task's destructor: free the buffer, forget the
    /// task if it is this one, and drop the reference count.
    pub fn destroy(&mut self, task: u32, host: &mut impl SparkHost) {
        if self.buffer.take().is_some() {
            host.free_buffer();
        }
        if self.task == task {
            self.task = 0;
        }
        self.refs = self.refs.wrapping_sub(1);
    }
}
