use sonic_advance_assets::transfer::{
    audit_capture, audit_transcript, source_evidence, Command, Direction, Exchange,
    MAX_PAYLOAD_BYTES,
};

fn exchanges(pairs: &[(u32, u32)]) -> Vec<Exchange> {
    pairs
        .iter()
        .map(|&(host_word, garden_word)| Exchange {
            host_word,
            garden_word,
        })
        .collect()
}

#[test]
fn receive_retries_overwrite_and_rejected_sequences_match_exact_asoe_thumb_dispatch() {
    let capture = exchanges(&[
        (0x0300a52a, 0x0300a52a),
        (0xbbaa0000, 0xbbaa0000),
        (0xffee0200, 0),
        (0x12340000, 0x12340000),
        (0xddcc0100, 0xddcc0100),
        (0xffee0200, 0xffee0200),
        (0x6f616843, 0x34124b43),
    ]);
    let report = audit_transcript(&capture).unwrap();
    assert_eq!(report.command.direction, Direction::ToGarden);
    assert_eq!(report.command.opaque, 0xa5);
    assert_eq!(report.payload, [0x34, 0x12, 0xcc, 0xdd, 0xee, 0xff]);
    assert_eq!(
        (
            report.retried_words,
            report.rejected_words,
            report.completion_input
        ),
        (1, 1, 0x1234)
    );
}
#[test]
fn native_send_repeats_previous_word_until_exact_echo_and_reports_final_input() {
    let capture = exchanges(&[
        (0x0300c422, 0x0300c422),
        (0x26, 0xbbaa0000),
        (0x9999, 0xbbaa0000),
        (0xbbaa0000, 0xddcc0100),
        (0xddcc0100, 0xffee0200),
        (0xffee0200, 0x34124b43),
    ]);
    let report = audit_transcript(&capture).unwrap();
    assert_eq!(report.command.direction, Direction::FromGarden);
    assert_eq!(report.payload, [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    assert_eq!(
        (
            report.retried_words,
            report.rejected_words,
            report.completion_input
        ),
        (1, 0, 0x1234)
    );
}

#[test]
fn malformed_or_incomplete_transcripts_never_produce_a_payload_report() {
    assert!(audit_transcript(&[]).is_err());
    for command in [0, 0x2a, 0x22, 0x01000026, 0x0104002a] {
        assert!(Command::parse(command).is_err());
    }
    let valid = exchanges(&[
        (0x0100002a, 0x0100002a),
        (0x12340000, 0x12340000),
        (0x6f616843, 0x4b43),
    ]);
    for n in 0..valid.len() {
        assert!(audit_transcript(&valid[..n]).is_err());
    }
    for i in 0..valid.len() {
        let mut bad = valid.clone();
        bad[i].garden_word ^= 1;
        assert!(audit_transcript(&bad).is_err());
    }
    let mut extra = valid.clone();
    extra.push(valid[1]);
    assert!(audit_transcript(&extra).is_err());
    let mut over = valid.clone();
    over.insert(
        2,
        Exchange {
            host_word: 0xabcd0100,
            garden_word: 0xabcd0100,
        },
    );
    assert!(audit_transcript(&over).is_err());
    assert!(audit_capture(0x01000022, &exchanges(&[(0x26, 0xabcd0100)])).is_err());
    assert!(audit_capture(0x0100002a, &exchanges(&[(0x0101002a, 0x0101002a)])).is_err());
    let mut send = exchanges(&[
        (0x26, 0xbbaa0000),
        (0x9999, 0xbbaa0001),
        (0xbbaa0000, 0x4b43),
    ]);
    assert!(audit_capture(0x01000022, &send).is_err());
    send[1].garden_word = 0xbbaa0000;
    assert!(audit_capture(0x01000022, &send).is_ok());
}

#[test]
fn full_save_tail_keeps_high_sequence_and_payload_bytes_in_original_order() {
    let command = Command::parse(0x0004ff2a).unwrap();
    assert_eq!(command.payload_bytes(), MAX_PAYLOAD_BYTES);
    let mut words = Vec::new();
    let mut expected = Vec::new();
    for i in 0..1024u16 {
        let pair = (i ^ 0x7fe3).to_le_bytes();
        let word = (u32::from(u16::from_le_bytes(pair)) << 16) | u32::from(i.swap_bytes());
        words.push(Exchange {
            host_word: word,
            garden_word: word,
        });
        expected.extend(pair);
    }
    words.push(Exchange {
        host_word: 0x6f616843,
        garden_word: 0x34124b43,
    });
    assert_eq!(
        audit_capture(command.raw, &words).unwrap().payload,
        expected
    );
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn authentic_embedded_irq_and_save_copy_extents_match() {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let spans = source_evidence(&rom).unwrap();
    let expected = [
        (
            0x43c,
            0x100,
            "cd0c28639235fd2368b11283e9e724fa6eb4e90b4f4a43527c5fd682f2b95647",
        ),
        (
            0x630,
            0x2d0,
            "6ff9c87bce327be76e4a2304add8cf73a4d3dcc8359457a1f5097688a0ba6c32",
        ),
        (
            0x900,
            0x58,
            "4b3f1f04dd060c13e3eb4b3a71299d19b89305a3750ca66e787969278fadc18c",
        ),
        (
            0x958,
            0xb8,
            "94b7c0fef1a18e05722211e9ebf484747b3a884aac937bcba5e550c1d4c74430",
        ),
        (
            0xa10,
            0x4c,
            "052ca59674deb6b49c3b43e9e3210801db56b0ab52263e15b3198e0f6f12ff9c",
        ),
        (
            0xa5c,
            0xb0,
            "e0825ceea59efd46d117ead9aed9b6becace77edf523c7cc4b466405e3991d13",
        ),
        (
            0x2c02,
            0x88,
            "fc1e6472ac4ffbbf46cea8e095ddcb7c018c55e732ed81bf820da1bfbad25cb3",
        ),
    ];
    assert_eq!(spans.len(), expected.len());
    for (s, (off, len, sha)) in spans.iter().zip(expected) {
        assert_eq!(
            (s.payload_offset, s.bytes, s.sha256.as_str()),
            (off, len, sha)
        );
    }
}
