use sonic_pinball_party::{
    catalog,
    game::{
        self,
        board::{NativeBoard, SCREEN_BYTES, SCREEN_HEIGHT, SCREEN_WIDTH},
    },
    runtime_host, sha256,
};
use std::{error::Error, fs::File, io::Read, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn read_rom(path: &Path) -> Result<Vec<u8>> {
    let mut rom = Vec::new();
    File::open(path)?
        .take((catalog::ROM_LEN + 1) as u64)
        .read_to_end(&mut rom)?;
    catalog::validate_rom(&rom)?;
    Ok(rom)
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().and_then(|s| s.to_str()).unwrap_or("help");
    if matches!(command, "help" | "--help") {
        println!("A3VE Rust source port — incomplete\nspp-game status ROM\nspp-game verify ROM\nspp-game run ROM\nspp-game board ROM FRAMES [--headless]\nboard is a native-background diagnostic with manual Up/Down scrolling. run stops at the first unported dependency. No MMO state or native saves are changed.");
        return Ok(());
    }
    if args.len() < 2 {
        return Err("missing ROM; use --help".into());
    }
    let rom = read_rom(Path::new(&args[1]))?;
    match command {
        "status" if args.len() == 2 => println!(
            "{}",
            serde_json::json!({
                "sourceSha256":catalog::ROM_SHA256,"wholeGamePorted":false,
                "runtime":"Rust source-level port in progress; native ROM instructions are not executed",
                "nativeInitialCallback":format!("0x{:08X}",game::INITIAL_CALLBACK),
                "startupCallbackWrapperPorted":true,
                "startupDependenciesPorted":false,
                "nativeBoardRasterSha256":game::board::BOARD_RGBA_SHA256,
                "missing":["startup/menu callbacks","complete ball/flipper/collision simulation","table events/scoring/progression","sprite assembly/animation","audio execution","main saves/transfer","complete Tiny Garden gameplay"]
            })
        ),
        "run" if args.len() == 2 => {
            let mut callbacks = game::control::CallbackStack::default();
            callbacks.push(game::control::CallbackId(game::INITIAL_CALLBACK), 0);
            game::dispatch_ported_callback(
                &mut callbacks,
                &mut game::control::CallbackStack::default(),
                &mut game::StartupPointer::default(),
                &mut game::startup::RejectUnportedStartupCalls,
            )?;
        }
        "verify" if args.len() == 2 => {
            println!(
                "{}",
                serde_json::to_string(&game::provenance::verify_rom(&rom)?)?
            );
        }
        "board" if args.len() == 3 || args.len() == 4 => {
            let frames: u64 = args[2]
                .to_str()
                .ok_or("frame count must be numeric")?
                .parse()?;
            if frames == 0 || frames > 36000 {
                return Err("frame budget must be 1..36000".into());
            }
            let headless = if args.len() == 4 {
                if args[3] != "--headless" {
                    return Err("unknown option".into());
                }
                true
            } else {
                false
            };
            let board = NativeBoard::load(&rom)?;
            if headless {
                let mut frame = vec![0; SCREEN_BYTES];
                for _ in 0..frames {
                    board.viewport(0, 0, &mut frame)?;
                }
                println!(
                    "{}",
                    serde_json::json!({"mode":"native-board-diagnostic","width":SCREEN_WIDTH,"height":SCREEN_HEIGHT,"frames":frames,"sourceSha256":catalog::ROM_SHA256,"rgbaSha256":sha256(&frame),"wholeGamePorted":false})
                );
            } else {
                let mut y = 0usize;
                let report = runtime_host::run(
                    "Sonic Pinball Party — native board diagnostic",
                    frames,
                    |context, out| {
                        if context.button_mask & 0x40 != 0 {
                            y = y.saturating_sub(2);
                        }
                        if context.button_mask & 0x80 != 0 {
                            y = (y + 2).min(512 - SCREEN_HEIGHT);
                        }
                        board.viewport(0, y, out).map_err(|e| e.to_string())
                    },
                )?;
                println!("{report:?}");
            }
        }
        _ => return Err("incorrect command/arguments; use --help".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("spp-game: {error}");
        std::process::exit(1);
    }
}
