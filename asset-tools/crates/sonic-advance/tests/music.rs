use sonic_advance_assets::{
    binary,
    music::{asoe_song, direct_sample, trace, Command},
};
#[test]
fn rejects_truncation_unknown_commands_and_zero_time_cycles() {
    for bytes in [
        &[0xb3, 0, 0][..],
        &[0xb2, 0, 0, 0, 2],
        &[0xb2, 0, 0, 0, 8],
        &[0xb3, 0, 0, 0, 8],
        &[0xce],
        &[0xd0, 60, 90],
        &[0x7f],
        &[0xcd, 99],
    ] {
        assert!(trace(bytes, 0, 3).is_err(), "{bytes:?}");
    }
}
#[test]
fn pattern_and_counter_repeat_share_running_note_parameters() {
    let bytes = [
        0xb3, 12, 0, 0, 8, 0xb5, 2, 0, 0, 0, 8, 0xb1, 0xd4, 37, 124, 0x86, 0xb4,
    ];
    let events = trace(&bytes, 0, 12).unwrap();
    assert_eq!(
        events.iter().map(|e| e.tick).collect::<Vec<_>>(),
        [0, 6, 12]
    );
    assert_eq!(events[2].command, Command::Fine);
    let notes = trace(&[0xd4, 37, 124, 0x86, 38, 0x86, 0xda, 0x86, 0xb1], 0, 12).unwrap();
    assert_eq!(
        notes[2].command,
        Command::Note {
            key: 38,
            velocity: 124,
            gate: 11,
            voice: 0
        }
    );
}
#[test]
fn rejects_invalid_wave_encoding_length_or_loop() {
    let mut rom = [0; 32];
    rom[4..8].copy_from_slice(&0x0800000cu32.to_le_bytes());
    rom[16..20].copy_from_slice(&1024u32.to_le_bytes());
    rom[24] = 4;
    assert_eq!(direct_sample(&rom, 0, 0).unwrap().signed_pcm.len(), 4);
    rom[15] = 0x40;
    rom[20] = 4;
    assert!(direct_sample(&rom, 0, 0).is_err());
    rom[15] = 0;
    rom[12] = 1;
    assert!(direct_sample(&rom, 0, 0).is_err());
    rom[12] = 0;
    rom[24] = 5;
    assert!(direct_sample(&rom, 0, 0).is_err());
}
#[test]
#[ignore = "requires the exact private SONIC_ADVANCE_ASOE_ROM"]
fn opening_matches_pinned_score_and_wave_matches_independent_hash() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let song = asoe_song(&rom, 1).unwrap();
    let events = trace(&rom, song.track_offsets[0], 882).unwrap();
    let notes: Vec<_> = events
        .iter()
        .filter_map(|e| match e.command {
            Command::Note {
                key,
                velocity,
                gate,
                voice,
            } => Some((e.tick, key, velocity, gate, voice)),
            _ => None,
        })
        .collect();
    assert_eq!(
        notes,
        [
            (864, 37, 124, 5, 127),
            (870, 37, 108, 5, 127),
            (876, 37, 108, 5, 127),
            (882, 37, 108, 11, 127)
        ]
    );
    assert!(events.iter().any(|e| e.tick == 864
        && e.command
            == Command::Control {
                opcode: 0xbb,
                value: 84
            }));
    let sample = direct_sample(&rom, song.voicegroup_offset.unwrap(), 70).unwrap();
    assert_eq!(
        (
            sample.pitch_1024,
            sample.loop_start,
            sample.signed_pcm.len()
        ),
        (10764288, None, 5312)
    );
    assert_eq!(
        binary::sha256(sample.signed_pcm),
        "0f364f8aaf3c13e50256a7c715c60a3ba3e4009f69ef9a8979a1963f85789a29"
    );
}

#[test]
#[ignore = "requires the exact private SONIC_ADVANCE_ASOE_ROM"]
fn every_native_track_runs_to_its_end_or_tick_4096() {
    use sonic_advance_assets::tables;
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    binary::verify_asoe(&rom).unwrap();
    let mut tracks = 0;
    let mut songs = 0;
    let mut errors = Vec::new();
    for (index, entry) in tables::song_table(&rom, 0x6d76ac, 308, 308)
        .unwrap()
        .into_iter()
        .enumerate()
    {
        let song = tables::song_header(&rom, entry.header_offset.unwrap(), 16).unwrap();
        if !song.track_offsets.is_empty() {
            songs += 1;
        }
        for (track, offset) in song.track_offsets.into_iter().enumerate() {
            tracks += 1;
            if let Err(e) = trace(&rom, offset, 4096) {
                errors.push((index, track + 1, e));
            }
        }
    }
    assert_eq!((songs, tracks), (180, 574));
    assert!(errors.is_empty(), "{errors:?}");
}
