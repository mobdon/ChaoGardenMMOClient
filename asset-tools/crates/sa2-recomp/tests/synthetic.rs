#![allow(unused_assignments, unreachable_code, unused_variables, unused_mut, clippy::all)]
mod g {
    include!(concat!(env!("OUT_DIR"), "/synthetic.rs"));
}
use sa2_recomp::rt::{Cpu, Func, Mem};

fn dispatch(a: u32) -> Option<Func> {
    match a {
        0x8000_3000 => Some(g::f_80003000),
        0x8000_3040 => Some(g::f_80003040),
        0x8000_3048 => Some(g::f_80003048),
        _ => None,
    }
}

fn setup() -> (Cpu, Mem) {
    let mut c = Cpu::default();
    c.rt.generated = dispatch;
    c.r[1] = 0x8100_0000;
    (c, Mem::default())
}

#[test]
fn loop_call_and_stack() {
    let (mut c, mut m) = setup();
    c.lr = 0xDEAD_BEEF;
    sa2_recomp::rt::call(&mut c, &mut m, 0x8000_3000);
    assert_eq!(c.r[3], 110); // (10 + 9 + ... + 1) * 2
    assert_eq!(c.r[1], 0x8100_0000);
    assert_eq!(c.lr, 0xDEAD_BEEF);
    assert_eq!(c.ctr, 0);
}

#[test]
fn override_replaces_function() {
    let (mut c, mut m) = setup();
    c.rt.set_override(0x8000_3040, |c, _| c.r[3] = 7);
    sa2_recomp::rt::call(&mut c, &mut m, 0x8000_3000);
    assert_eq!(c.r[3], 7);
}

#[test]
fn single_float_round_trip() {
    let (mut c, mut m) = setup();
    m.w32(0x8000_8000, 1.5f32.to_bits());
    c.r[3] = 0x8000_8000;
    sa2_recomp::rt::call(&mut c, &mut m, 0x8000_3048);
    assert_eq!(f32::from_bits(m.r32(0x8000_8004)), 3.0);
    assert_eq!(c.f[1].ps1, 3.0);
}
