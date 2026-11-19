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
        println!("A3VE Rust source port — incomplete\nspp-game status ROM\nspp-game verify ROM\nspp-game run ROM\nspp-game garden-board ROM ROUND SEED\nspp-game board ROM FRAMES [--headless]\ngarden-board runs native puzzle setup on explicit synthetic RAM, then fails at the first unported frame dependency. board is a native-background diagnostic with manual Up/Down scrolling. run stops at the first unported dependency. No MMO state or native saves are changed.");
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
                "introInitializationAndAuxiliaryCallbacksPorted":true,
                "startupDependenciesPorted":false,
                "gardenBoardSetupPorted":true,
                "wholeGardenPorted":false,
                "gardenFrameFirstMissingAddress":"0x02001420",
                "nativeBoardRasterSha256":game::board::BOARD_RGBA_SHA256,
                "missing":["complete boot/screen wiring","complete table rules and progression","complete camera/layer/sprite presentation","complete sequencer/mixer/audio playback","main saves/transfer","complete Tiny Garden gameplay"]
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
        "garden-board" if args.len() == 4 => {
            let round: u8 = args[2].to_str().ok_or("round must be numeric")?.parse()?;
            if round > 3 {
                return Err("native round must be 0..3".into());
            }
            let seed: u32 = args[3].to_str().ok_or("seed must be numeric")?.parse()?;
            let payload = catalog::extract_garden(&rom)?.bytes;
            use game::{
                frame::FrameMemory,
                garden_runtime::{RejectBackend, Runtime},
                memory::{MemoryImage, Region},
            };
            let mut memory = MemoryImage::new(
                &rom,
                [
                    (0x03000000, 0x7000),
                    (0x04000000, 0x400),
                    (0x05000000, 0x400),
                    (0x06000000, 0x18000),
                    (0x07000000, 0x400),
                ]
                .into_iter()
                .map(|(base, length)| Region {
                    base,
                    bytes: vec![0; length],
                    writable: true,
                })
                .collect(),
            )?;
            memory.write_u8(0x03001ad1, round)?;
            memory.write_bytes(0x030013a8, &seed.to_le_bytes())?;
            let mut runtime = Runtime::new(&payload, RejectBackend)?;
            runtime.call(&mut memory, 0xd504, &[])?;
            let frame = runtime.call(&mut memory, 0xd7cc, &[]);
            let frame_stop_address = match &frame {
                Err(game::frame::FrameError::Dependency { call, .. })
                | Err(game::frame::FrameError::UnportedNativeCall(call)) => {
                    Some(format!("0x{:08X}", call.entry_offset))
                }
                _ => None,
            };
            println!(
                "{}",
                serde_json::json!({
                    "mode":"native-garden-board-probe",
                    "sourceSha256":catalog::ROM_SHA256,
                    "payloadSha256":sonic_pinball_party::garden::PAYLOAD_SHA256,
                    "initialState":"explicit synthetic RAM with selected round and seed; no reset or loaded save claim",
                    "round":round,"seed":seed,"setupSucceeded":true,
                    "callbackId":format!("0x{:08X}",memory.read_u32(game::garden_frame::GLOBALS)?),
                    "ramSha256":sha256(memory.bytes(0x03000000,0x7000)?),
                    "frameCompleted":frame.is_ok(),"frameStop":frame.as_ref().err().map(ToString::to_string),
                "frameStopAddress":frame_stop_address,
                    "wholeGardenPorted":false,"wholeGamePorted":false
                })
            );
            frame?;
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
