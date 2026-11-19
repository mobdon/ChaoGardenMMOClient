use sonic_advance_assets::music::mixer::*;
#[test]
fn refuses_unbounded_or_unknown_cues_before_reading() {
    for (song, frames) in [(12, 0), (12, MAX_FRAMES + 1), (0, 1)] {
        assert!(render(&[], song, frames).is_err());
    }
}
#[test]
#[ignore = "requires private verified ASOE"]
fn stage_and_ring_use_their_retail_voice_families() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    for song in [12, 13, 102] {
        let frames = if song == 102 { 120 } else { MAX_FRAMES };
        let r = render(&rom, song, frames).unwrap();
        assert_eq!(r.stereo.len(), frames * SAMPLES_PER_FRAME);
        assert!(r.stereo.iter().any(|p| p[0] != 0));
        assert!(r.peak_polyphony <= 8);
        eprintln!("{song}: {:?}, peak {}", r.notes, r.peak_polyphony);
        if song == 102 {
            assert_eq!(r.notes, [1, 0, 0, 0, 0]);
            assert!(r.stereo[176 * 100..].iter().all(|p| *p == [0, 0]));
        } else {
            assert!(r.notes[0] > 100 && r.notes[2] > 0 && r.notes[3] > 0 && r.notes[4] > 0);
            assert!(r.peak_polyphony >= 5);
            assert!(r.stereo.iter().any(|p| p[0] != p[1]));
        }
    }
}
