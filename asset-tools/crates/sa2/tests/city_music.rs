use sa2_asset_tools::city_music::{prolog, AdxTrigger, BgmStart, MusicCall};
use sha2::{Digest, Sha256};
use std::fmt::Write;

fn log_calls(out: &mut String, calls: Vec<MusicCall>) {
    for call in calls {
        match call {
            MusicCall::Fn80011df4 => out.push('F'),
            MusicCall::Fn80011fb0(file) => write!(out, "Q{file};").unwrap(),
            MusicCall::Play(file) => write!(out, "P{file};").unwrap(),
            MusicCall::StagePlay(file) => write!(out, "S{file};").unwrap(),
            MusicCall::SetVolume(level) => write!(out, "V{level};").unwrap(),
        }
    }
}

#[test]
fn matches_host_c_callback_traces() {
    let states = [0, 7, 9, 13, 16];
    let mut out = String::new();
    for variant in [0, 1] {
        for two_player in [false, true] {
            let (task, calls) = prolog(variant, two_player);
            if task.is_some() {
                out.push('T');
            }
            log_calls(&mut out, calls);
            out.push('|');
        }
    }
    for range_out in [false, true] {
        for short_city in [false, true] {
            write!(
                out,
                "{}|",
                u8::from(AdxTrigger::spawn(range_out, short_city).is_some())
            )
            .unwrap();
        }
    }
    for mode in 0..=4 {
        for state in states {
            for range_out in [false, true] {
                for inside in [false, true] {
                    let mut task = AdxTrigger { mode };
                    log_calls(&mut out, task.tick(range_out, inside, state));
                    write!(out, "{}|", task.mode).unwrap();
                }
            }
        }
    }
    for mode in 0..=4 {
        for state in states {
            for pred in [false, true] {
                log_calls(&mut out, AdxTrigger { mode }.destroy(state, pred));
                out.push('|');
            }
        }
    }
    for counter in 0..=12 {
        for state in states {
            for p in 0..4 {
                let mut task = BgmStart { counter };
                let modes = [
                    if p & 1 != 0 { 54 } else { 0 },
                    if p & 2 != 0 { 54 } else { 0 },
                ];
                log_calls(&mut out, task.tick(state, modes));
                write!(out, "{}|", task.counter).unwrap();
            }
        }
    }
    assert_eq!(out.len(), 1905);
    assert_eq!(
        format!("{:x}", Sha256::digest(out.as_bytes())),
        "e87cb5630e85a7d9ddcfdcdc46a9f34168d9992933b2af9db8c7ca6b35221917"
    );
}
