//! BIG THE CAT (object list row 23).
//!
//! stg42D.rel section 1 +0x1B7FC is the row's whole object function: it
//! calls `CheckRangeOut(task)` (0x800655F4) and returns whatever that says
//! (both branches of its compare reach the same epilogue). It installs no
//! exec, display or model and starts no task, so in the GameCube stage RELs
//! the object only exists until the range check removes it.
//!
//! The two routines right after it (0x1B824 and 0x1B878, a 16-slot table at
//! section 5 +0x1B160) are not BIG's: they are ORI's rocket table join and
//! leave (called only from ORI at 0x18B70, 0x18BC8 and 0x18E70), byte for
//! byte (relocation-masked) stg09D.rel 0x35A1C and 0x35A70, which `ori`
//! describes; they are left to ORI's owner.
use super::host::ObjectHost;

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[super::Routine {
    name: "BIG THE CAT",
    stg42d: 0x1b7fc,
    len: 0x28,
    masked_sha256: "b54688cc8faf95704168ed005f832d19ea498fa1f2c119e62fb71da28762d286",
}];

/// One frame of 0x1B7FC.
pub fn exec(host: &mut impl ObjectHost) {
    host.range_out();
}
