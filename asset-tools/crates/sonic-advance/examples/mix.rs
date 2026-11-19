use sonic_advance_assets::{
    binary,
    music::mixer::{self, SAMPLE_RATE},
};
use std::{
    error::Error,
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
};
fn main() -> Result<(), Box<dyn Error>> {
    let a: Vec<_> = std::env::args_os().skip(1).collect();
    if a.len() != 4 {
        return Err("usage: mix ROM SONG FRAMES NEW.wav".into());
    }
    let rom = binary::read_bounded(Path::new(&a[0]), binary::ROM_LEN)?;
    let output = mixer::render(
        &rom,
        a[1].to_str().ok_or("invalid song")?.parse()?,
        a[2].to_str().ok_or("invalid frame count")?.parse()?,
    )?;
    let bytes = u32::try_from(output.stereo.len() * 4)?;
    let mut w = BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&a[3])?,
    );
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + bytes).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&[1, 0, 2, 0])?;
    w.write_all(&SAMPLE_RATE.to_le_bytes())?;
    w.write_all(&(SAMPLE_RATE * 4).to_le_bytes())?;
    w.write_all(&[4, 0, 16, 0])?;
    w.write_all(b"data")?;
    w.write_all(&bytes.to_le_bytes())?;
    for [l, r] in &output.stereo {
        w.write_all(&l.to_le_bytes())?;
        w.write_all(&r.to_le_bytes())?;
    }
    w.flush()?;
    eprintln!(
        "{} stereo frames; note starts [PCM, square1, square2, wave, noise] {:?}; peak {} voices",
        output.stereo.len(),
        output.notes,
        output.peak_polyphony
    );
    Ok(())
}
