use sa2_asset_tools::{
    manifest::sha256,
    story::{self, Action, Cursor, EntryType},
};
use std::{fmt::Write, fs, path::PathBuf};
#[test]
#[ignore = "requires verified local Steam executable and SA Tools PC_SA2/ADVERTISE.ini"]
fn native_sequences_match_sa_tools_split_and_ordered_acknowledgments() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap());
    let config = fs::read_to_string(cfg.join("ADVERTISE.ini")).unwrap();
    assert_eq!(
        sha256(config.as_bytes()),
        "786f9083923d5f2dbec73fdb0be284154ced16c433b335386bba0af89272a3a0"
    );
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let data = story::read(&exe, &config).unwrap();
    assert_eq!(data.len(), 15);
    let mut semantic = String::new();
    let mut count = 0;
    let mut actions = 0;
    for (name, seq) in &data {
        writeln!(semantic, "{name}").unwrap();
        let mut expected = Vec::new();
        for (i, e) in seq.entries().iter().enumerate() {
            assert_eq!(e.rva, seq.start_rva + 12 * i as u32);
            count += 1;
            let events: Vec<_> = e
                .event_words
                .iter()
                .copied()
                .take_while(|x| *x >= 0)
                .collect();
            writeln!(
                semantic,
                "{}|{}|{}|{}",
                e.kind as i8,
                e.character,
                e.level,
                events
                    .iter()
                    .map(i16::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .unwrap();
            match e.kind {
                EntryType::Event => expected.extend(
                    events
                        .into_iter()
                        .map(|event| (e.rva, Action::Event { event })),
                ),
                EntryType::Level => expected.push((
                    e.rva,
                    Action::Level {
                        character: e.character,
                        level: e.level,
                    },
                )),
                EntryType::Credits => expected.push((e.rva, Action::Credits)),
                EntryType::End => panic!("premature end"),
            }
        }
        let end = &seq.terminator;
        assert_eq!(end.rva, seq.start_rva + 12 * seq.entries().len() as u32);
        assert_eq!(
            (end.kind, end.character, end.level, end.event_words),
            (EntryType::End, 0, 0, [0; 4])
        );
        let mut cursor = Cursor::new(seq);
        let mut other = Cursor::new(seq);
        let foreign = other.dispatch().unwrap();
        assert!(cursor.acknowledge(&foreign.token).is_err());
        for (rva, action) in expected {
            let d = cursor.dispatch().unwrap();
            assert_eq!(d.source_rva, rva);
            assert_eq!(d.action, action);
            assert_eq!(cursor.dispatch().unwrap(), d);
            assert!(cursor.acknowledge(&foreign.token).is_err());
            cursor.acknowledge(&d.token).unwrap();
            assert!(cursor.acknowledge(&d.token).is_err());
            actions += 1;
        }
        assert!(cursor.finished());
        assert!(cursor.dispatch().is_none());
    }
    assert_eq!((count, actions), (143, 171));
    assert_eq!(
        sha256(semantic.as_bytes()),
        "4f362db04ae1dd46801d9db9044f6e825e6d5783b31c28e554d457630548c7ed"
    );
    let hero = &data["Hero Story Sequence"];
    assert_eq!(hero.entries()[43].kind, EntryType::Credits);
    assert_eq!(hero.entries()[44].event_words, [429, -1, -1, -1]);
    let last = &data["Last Story Sequence"];
    assert_eq!(last.entries()[4].event_words, [206, 207, -1, 0]);
    assert_eq!(last.entries()[0].event_words, [200, 201, 602, 203]);
}
