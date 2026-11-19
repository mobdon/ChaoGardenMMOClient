use sonic_advance_assets::animation::*;

fn words(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

#[test]
fn command_representation_preserves_all_observed_fields_and_exact_bytes() {
    // Synthetic values, unrelated to a native game script. Opaque arguments
    // intentionally include sign bits, nontrivial packed fields, and zeros.
    let source = words(&[
        -1,
        9,
        2,
        -2,
        7,
        0x1234_0020,
        -5,
        33,
        -6,
        1,
        -17,
        -7,
        0x7fff_8000,
        -8,
        -1,
        0,
        -10,
        3,
        4,
        5,
        -11,
        2,
        -12,
        -2,
        0,
        0x1234,
        19,
        0,
        -4,
    ]);
    let parsed = parse_commands(&source, MAX_COMMANDS, source.len()).unwrap();
    assert_eq!(parsed.consumed, source.len());
    let expected_kinds = [
        CommandKind::Tiles,
        CommandKind::Palette,
        CommandKind::PlaySound,
        CommandKind::Hitbox,
        CommandKind::Translate,
        CommandKind::Opaque8,
        CommandKind::Opaque10,
        CommandKind::SetPriority,
        CommandKind::Opaque12,
        CommandKind::ShowFrame,
        CommandKind::ShowFrame,
        CommandKind::End,
    ];
    assert_eq!(
        parsed
            .commands
            .iter()
            .map(|command| command.kind)
            .collect::<Vec<_>>(),
        expected_kinds
    );
    assert_eq!(parsed.commands[1].operands, [7, 0x1234_0020]);
    assert_eq!(parsed.commands[3].operands, [1, (-17_i32) as u32]);
    assert_eq!(parsed.commands[5].operands, [u32::MAX, 0]);
    assert_eq!(parsed.commands[9].raw_opcode, 0);
    assert_eq!(parsed.commands[9].operands, [0x1234]);
    let mut rebuilt = Vec::new();
    for command in &parsed.commands {
        assert_eq!(command.byte_offset, rebuilt.len());
        rebuilt.extend_from_slice(&command.raw_opcode.to_le_bytes());
        for operand in &command.operands {
            rebuilt.extend_from_slice(&operand.to_le_bytes());
        }
    }
    assert_eq!(rebuilt, source);
}

#[test]
fn terminals_stop_linear_parsing_without_following_untrusted_targets() {
    for terminal in [vec![-3, i32::MAX], vec![-9, -1], vec![-4]] {
        let prefix = words(&terminal);
        let mut source = prefix.clone();
        source.extend_from_slice(&(-13_i32).to_le_bytes());
        source.extend_from_slice(&[0xff, 0]);
        let result = parse_commands(&source, 1, prefix.len()).unwrap();
        assert_eq!(result.consumed, prefix.len());
        assert!(result.commands[0].kind.is_terminal());
        assert_eq!(
            result.commands[0].operands,
            terminal[1..]
                .iter()
                .map(|&word| word as u32)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn incomplete_commands_unknown_opcodes_and_missing_terminals_are_rejected() {
    for command in [
        vec![-1, 0, 1],
        vec![-2, 2, 3],
        vec![-3, 4],
        vec![-4],
        vec![-5, 6],
        vec![-6, 7, 8],
        vec![-7, 9],
        vec![-8, 10, 11],
        vec![-9, 12],
        vec![-10, 13, 14, 15],
        vec![-11, 16],
        vec![-12, 17],
        vec![18, 19],
    ] {
        let source = words(&command);
        for end in 0..source.len() {
            assert!(
                parse_commands(&source[..end], 32, 1024).is_err(),
                "{command:?} prefix {end}"
            );
        }
    }
    for opcode in [-13, -14, -100, i32::MIN] {
        assert!(parse_commands(&words(&[opcode, -4]), 32, 1024).is_err());
    }
    assert!(parse_commands(&words(&[7, 3]), 32, 1024).is_err());
}

#[test]
fn caller_and_hard_budgets_apply_before_terminal_is_consumed() {
    let source = words(&[1, 2, -4]);
    assert!(parse_commands(&source, 0, source.len()).is_err());
    assert!(parse_commands(&source, 1, source.len()).is_err());
    assert!(parse_commands(&source, 2, source.len() - 1).is_err());
    assert!(parse_commands(&source, 2, 0).is_err());
    assert_eq!(
        parse_commands(&source, 2, source.len())
            .unwrap()
            .commands
            .len(),
        2
    );
    let mut long = Vec::new();
    for _ in 0..MAX_COMMANDS {
        long.extend_from_slice(&words(&[0, 0]));
    }
    long.extend_from_slice(&words(&[-4]));
    assert!(parse_commands(&long, usize::MAX, usize::MAX).is_err());
}
