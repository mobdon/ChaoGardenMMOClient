//! Each hook driven through `rt::call`, as recompiled code reaches it.
use sa2_asset_tools::input::PadStatus;
use sa2_boot::*;
use sa2_recomp::rt::{call, Cpu, Mem};
use sa2_runtime::dvd::MemFiles;

fn setup() -> (Cpu, Mem) {
    let mut c = Cpu::default();
    let mut m = Mem::default();
    let mut f = MemFiles::default();
    f.0.insert(
        "resource/gd_PC/stg11D.rel".into(),
        (0u8..64).collect::<Vec<_>>(),
    );
    install_all(&mut c, &mut m, Box::new(f));
    (c, m)
}

fn put_str(m: &mut Mem, a: u32, s: &str) {
    m.load(a, s.as_bytes());
    m.w8(a + s.len() as u32, 0);
}

#[test]
fn dvd_open_and_read() {
    let (mut c, mut m) = setup();
    put_str(&mut m, 0x8000_1000, "/STG11D.rel");
    c.r[3] = 0x8000_1000;
    c.r[4] = 0x8000_2000;
    call(&mut c, &mut m, dvd::DVD_OPEN);
    assert_eq!(c.r[3], 1);
    assert_eq!(m.r32(0x8000_2034), 64);
    // DVDReadPrio(fi, 0x80003000, 8, 16, 2).
    c.r[3..8].copy_from_slice(&[0x8000_2000, 0x8000_3000, 8, 16, 2]);
    call(&mut c, &mut m, dvd::DVD_READ_PRIO);
    assert_eq!(c.r[3], 8);
    assert_eq!(m.r32(0x8000_3000), u32::from_be_bytes([16, 17, 18, 19]));

    put_str(&mut m, 0x8000_1000, "nope.bin");
    c.r[3] = 0x8000_1000;
    call(&mut c, &mut m, dvd::DVD_OPEN);
    assert_eq!(c.r[3], 0);
}

fn mark_callback(c: &mut Cpu, m: &mut Mem) {
    // Record (result, fileinfo) where the test can see them.
    m.w32(0x8000_4000, c.r[3]);
    m.w32(0x8000_4004, c.r[4]);
    c.r[3] = 0xDEAD;
}

#[test]
fn dvd_async_read_runs_callback_and_keeps_registers() {
    let (mut c, mut m) = setup();
    c.rt.set_override(0x8000_5000, mark_callback);
    put_str(&mut m, 0x8000_1000, "stg11d.rel");
    c.r[3] = 0x8000_1000;
    c.r[4] = 0x8000_2000;
    call(&mut c, &mut m, dvd::DVD_OPEN);
    c.r[3..9].copy_from_slice(&[0x8000_2000, 0x8000_3000, 4, 0, 0x8000_5000, 2]);
    c.r[31] = 77;
    call(&mut c, &mut m, dvd::DVD_READ_ASYNC_PRIO);
    assert_eq!(c.r[3], 1);
    assert_eq!(c.r[31], 77);
    assert_eq!(m.r32(0x8000_4000), 4);
    assert_eq!(m.r32(0x8000_4004), 0x8000_2000);
    assert_eq!(m.r32(0x8000_2000 + 0x0C), 0);
}

#[test]
fn fifo_stores_draw_at_copy_disp() {
    let (mut c, mut m) = setup();
    // VCD POS direct, VAT0 POS XYZ F32, one triangle, stored word by word.
    let mut s = vec![0x08, 0x50, 0, 0, 2, 0, 0x08, 0x70, 0, 0, 0, 9];
    s.extend([0x90, 0, 3]);
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        s.extend(v.to_be_bytes());
    }
    for b in &s {
        m.w8(gx::PIPE, *b);
    }
    assert!(gx::take_frames().is_empty());
    c.r[3] = 0x8010_0000;
    call(&mut c, &mut m, gx::GX_COPY_DISP);
    let f = gx::take_frames();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].0, 0x8010_0000);
    assert_eq!(f[0].1.draws.len(), 1);
}

#[test]
fn retrace_moves_clock_and_count() {
    let (mut c, mut m) = setup();
    call(&mut c, &mut m, clock::OS_GET_TIME);
    let t0 = (u64::from(c.r[3]) << 32) | u64::from(c.r[4]);
    call(&mut c, &mut m, vi::VI_WAIT_FOR_RETRACE);
    call(&mut c, &mut m, vi::VI_GET_RETRACE_COUNT);
    assert_eq!(c.r[3], 1);
    call(&mut c, &mut m, clock::OS_GET_TIME);
    let t1 = (u64::from(c.r[3]) << 32) | u64::from(c.r[4]);
    assert!(t1 - t0 > clock::TB_PER_FRAME);
    c.r[3] = 0x8030_0000;
    call(&mut c, &mut m, vi::VI_SET_NEXT_FRAME_BUFFER);
    assert_eq!(vi::next_frame_buffer(), 0x8030_0000);
}

#[test]
fn pad_read_and_clamp() {
    let (mut c, mut m) = setup();
    let mut p = [PadStatus::default(); 4];
    p[0].button = 0x0100;
    p[0].stick = [100, -5];
    p[1].err = -1;
    p[2].err = -1;
    p[3].err = -1;
    pad::set(p);
    call(&mut c, &mut m, pad::PAD_INIT);
    assert_eq!(c.r[3], 1);
    c.r[3] = 0x8000_6000;
    call(&mut c, &mut m, pad::PAD_READ);
    assert_eq!(m.r16(0x8000_6000), 0x0100);
    assert_eq!(m.r8(0x8000_6002) as i8, 100);
    assert_eq!(m.r8(0x8000_600C + 10) as i8, -1);
    c.r[3] = 0x8000_6000;
    call(&mut c, &mut m, pad::PAD_CLAMP);
    assert!((m.r8(0x8000_6002) as i8) < 100);
}

#[test]
fn audio_aram_round_trip_and_idle_regs() {
    let (mut c, mut m) = setup();
    call(&mut c, &mut m, audio::AR_INIT);
    assert_eq!(c.r[3], 0x4000);
    c.r[3] = 0x100;
    call(&mut c, &mut m, audio::AR_ALLOC);
    let at = c.r[3];
    m.load(0x8000_7000, &[1, 2, 3, 4]);
    c.r[3..7].copy_from_slice(&[0, 0x8000_7000, at, 4]);
    call(&mut c, &mut m, audio::AR_START_DMA);
    c.r[3..7].copy_from_slice(&[1, 0x8000_8000, at, 4]);
    call(&mut c, &mut m, audio::AR_START_DMA);
    assert_eq!(m.r32(0x8000_8000), 0x0102_0304);
    assert_eq!(m.r16(audio::DSP_REGS + 0xA), 0);
    m.w32(audio::AI_REGS, 5);
}
