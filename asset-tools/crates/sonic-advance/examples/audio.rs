use sonic_advance_assets::{binary, music};
use std::{
    error::Error,
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let arg = |i: usize| {
        args.get(i)
            .and_then(|v| v.to_str())
            .ok_or("missing numeric argument or command")
    };
    let rom = binary::read_bounded(
        Path::new(args.first().ok_or(
            "usage: audio ROM SONG trace TRACK TICK | audio ROM SONG sample VOICE OUTPUT.wav",
        )?),
        binary::ROM_LEN,
    )?;
    let song = music::asoe_song(&rom, arg(1)?.parse()?)?;
    match arg(2)? {
        "trace" if args.len() == 5 => {
            let track: usize = arg(3)?.parse()?;
            let offset = song
                .track_offsets
                .get(track.checked_sub(1).ok_or("tracks start at 1")?)
                .ok_or("track outside song")?;
            let events = music::trace(&rom, *offset, arg(4)?.parse()?)?;
            serde_json::to_writer_pretty(std::io::stdout().lock(), &events)?;
        }
        "sample" if args.len() == 5 => {
            let sample = music::direct_sample(
                &rom,
                song.voicegroup_offset.ok_or("empty voicegroup")?,
                arg(3)?.parse()?,
            )?;
            let count = u32::try_from(sample.signed_pcm.len())?;
            let rate = (u64::from(sample.pitch_1024) + 512) / 1024;
            let mut out = BufWriter::new(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&args[4])?,
            );
            out.write_all(b"RIFF")?;
            out.write_all(&(36 + count + (count % 2)).to_le_bytes())?;
            out.write_all(b"WAVEfmt ")?;
            out.write_all(&16_u32.to_le_bytes())?;
            out.write_all(&[1, 0, 1, 0])?;
            for _ in 0..2 {
                out.write_all(&(rate as u32).to_le_bytes())?;
            }
            out.write_all(&[1, 0, 8, 0])?;
            out.write_all(b"data")?;
            out.write_all(&count.to_le_bytes())?;
            for chunk in sample.signed_pcm.chunks(4096) {
                let transformed: Vec<_> = chunk.iter().map(|v| v ^ 0x80).collect();
                out.write_all(&transformed)?;
            }
            if count % 2 == 1 {
                out.write_all(&[0])?;
            }
            out.flush()?;
        }
        _ => return Err("expected trace TRACK TICK or sample VOICE OUTPUT.wav".into()),
    }
    Ok(())
}
