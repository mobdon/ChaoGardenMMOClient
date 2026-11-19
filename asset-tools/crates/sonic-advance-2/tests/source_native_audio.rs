use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_audio::{self as audio, Command},
};

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn render_actual_source_cues_and_report_hashes() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let evidence: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-audio-evidence.json")).unwrap();
    for &id in audio::SUPPORTED_CUES {
        let song = audio::song(&rom, id).unwrap();
        let complete = audio::render_cue(&rom, id).unwrap();
        let timing = complete.timing;
        let through = timing
            .track_loops
            .iter()
            .flatten()
            .map(|v| v.end_tick - 1)
            .max()
            .unwrap_or(4096);
        let mut traces = Vec::new();
        for &offset in &song.track_offsets {
            let events = audio::trace(rom.rom_bytes(), offset, through + 1).unwrap();
            let mut notes = Vec::<[u32; 5]>::new();
            let mut ties = Vec::<usize>::new();
            for event in events {
                match event.command {
                    Command::Note {
                        key,
                        velocity,
                        gate,
                        voice,
                    } if event.tick <= through => {
                        if gate == 0 {
                            ties.push(notes.len());
                        }
                        notes.push([
                            event.tick,
                            u32::from(key),
                            u32::from(velocity),
                            u32::from(gate),
                            u32::from(voice),
                        ]);
                    }
                    Command::EndTie { key } => {
                        if let Some(at) = ties.iter().rposition(|&i| notes[i][1] == u32::from(key))
                        {
                            let i = ties.remove(at);
                            notes[i][3] = event.tick - notes[i][0];
                        }
                    }
                    Command::Fine => {
                        for i in ties.drain(..) {
                            notes[i][3] = event.tick - notes[i][0];
                        }
                    }
                    _ => (),
                }
            }
            assert!(
                ties.is_empty(),
                "cue{id} has a tie spanning its score boundary"
            );
            traces.push(notes);
        }
        let render = complete.audio;
        let frames = render.stereo.len() / audio::mixer::SAMPLES_PER_FRAME;
        if let Some(end) = timing.loop_end_frame {
            assert_eq!(frames, end);
            assert!(timing.loop_start_frame.unwrap() < end);
        } else {
            assert!(frames > timing.fine_frame.unwrap());
        }
        assert_eq!(
            render.stereo.len(),
            frames * audio::mixer::SAMPLES_PER_FRAME
        );
        assert!(
            render.stereo.iter().flatten().any(|&v| v != 0),
            "cue{id} must be audible"
        );
        let pcm: Vec<u8> = render
            .stereo
            .iter()
            .flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes()))
            .collect();
        let mut trace_bytes = Vec::new();
        for track in &traces {
            trace_bytes.extend_from_slice(&(track.len() as u32).to_le_bytes());
            trace_bytes.extend(track.iter().flatten().flat_map(|v| v.to_le_bytes()));
        }
        let pcm_source = if id == 102 {
            let sample =
                audio::direct_sample(rom.rom_bytes(), song.voicegroup_offset.unwrap(), 2).unwrap();
            Some(
                serde_json::json!({"waveOffset":sample.wave_offset,"dataOffset":sample.data_offset,"length":sample.signed_pcm.len(),"pitch1024":sample.pitch_1024,"loopStart":sample.loop_start,"sha256":gba::sha256(sample.signed_pcm)}),
            )
        } else {
            None
        };
        if let Some(path) = std::env::var_os("A2NE_AUDIO_OUTPUT") {
            let root = std::path::PathBuf::from(path);
            assert!(
                root.components().any(|c| c.as_os_str() == ".local"),
                "Native audio must remain under .local"
            );
            std::fs::create_dir_all(&root).unwrap();
            let trace_target = root.join(format!("trace-{id}.json"));
            let trace_json = serde_json::to_vec(&traces).unwrap();
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&trace_target)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(&trace_json).unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    assert_eq!(std::fs::read(trace_target).unwrap(), trace_json)
                }
                Err(e) => panic!("Native trace output: {e}"),
            }
            let target = root.join(format!("cue-{id}.wav"));
            let wav = audio::wav(&render).unwrap();
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(&wav).unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    assert_eq!(std::fs::read(target).unwrap(), wav)
                }
                Err(e) => panic!("Native audio output: {e}"),
            }
        }
        let actual = serde_json::json!({"id":id,"frames":frames,"tracks":song.track_offsets.len(),"notes":render.notes,"peakPolyphony":render.peak_polyphony,"pcmBytes":pcm.len(),"pcmSha256":gba::sha256(&pcm),"scoreThrough":through,"scoreNotes":traces.iter().map(Vec::len).sum::<usize>(),"scoreSha256":gba::sha256(&trace_bytes),"pcmSource":pcm_source,"timing":timing});
        let expected = evidence["cues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap();
        // The score/sample fingerprints have independent source-byte checks.
        // Floating-point preview hashes are recorded, without asserting that
        // different targets' libm implementations produce identical rounding.
        for field in [
            "frames",
            "tracks",
            "notes",
            "peakPolyphony",
            "pcmBytes",
            "scoreThrough",
            "scoreNotes",
            "scoreSha256",
            "pcmSource",
            "timing",
        ] {
            assert_eq!(actual[field], expected[field], "cue{id}: {field}");
        }
        println!("AUDIO {actual}");
    }
}

#[test]
fn pcm_reader_validates_encoding_pitch_loop_bounds_and_sample_extent() {
    let mut valid = [0; 64];
    valid[4..8].copy_from_slice(&0x0800_0020u32.to_le_bytes());
    valid[36..40].copy_from_slice(&1024u32.to_le_bytes());
    valid[44..48].copy_from_slice(&16u32.to_le_bytes());
    let sample = audio::direct_sample(&valid, 0, 0).unwrap();
    assert_eq!(
        (
            sample.wave_offset,
            sample.data_offset,
            sample.signed_pcm.len()
        ),
        (32, 48, 16)
    );
    assert_eq!(sample.loop_start, None);
    for (at, value) in [(32, 1), (36, 0), (44, 0), (44, 17), (4, 0x0800_0021)] {
        let mut bytes = valid;
        bytes[at..at + 4].copy_from_slice(&u32::to_le_bytes(value));
        assert!(audio::direct_sample(&bytes, 0, 0).is_err());
    }
    let mut looping = valid;
    looping[32..36].copy_from_slice(&0x4000_0000u32.to_le_bytes());
    looping[40..44].copy_from_slice(&15u32.to_le_bytes());
    assert_eq!(
        audio::direct_sample(&looping, 0, 0).unwrap().loop_start,
        Some(15)
    );
    looping[40..44].copy_from_slice(&16u32.to_le_bytes());
    assert!(audio::direct_sample(&looping, 0, 0).is_err());
    assert!(audio::direct_sample(&valid, usize::MAX, 1).is_err());
    assert!(audio::direct_sample(&valid[..47], 0, 0).is_err());
}

#[test]
fn sequence_rejects_truncated_unknown_nested_and_runaway_inputs() {
    for bytes in [
        vec![],
        vec![0xbd],
        vec![0xb2],
        vec![0xcd, 0xff],
        vec![0xc6],
        vec![1],
        vec![0xff, 1, 1, 127],
    ] {
        assert!(audio::trace(&bytes, 0, 128).is_err());
    }
    assert_eq!(audio::trace(&[0xb1], 0, 128).unwrap().len(), 1);
    let infinite = [0xb2, 0, 0, 0, 8];
    assert!(audio::trace(&infinite, 0, 128).is_err());
    let nesting = [0xb3, 0, 0, 0, 8];
    assert!(audio::trace(&nesting, 0, 128).is_err());
}
