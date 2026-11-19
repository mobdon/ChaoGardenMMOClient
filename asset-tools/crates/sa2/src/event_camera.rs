//! BATTLE big-endian motion banks, restricted to one-part linear Ninja cameras.
use crate::{Error, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CameraAttributes {
    pub position: [f32; 3],
    pub direction_z: [f32; 3],
    pub roll_bams: i32,
    pub fov_bams: i32,
    pub near_clip: f32,
    pub far_clip: f32,
    pub direction_x: [f32; 3],
    pub direction_y: [f32; 3],
}
#[derive(Debug, Serialize)]
pub struct CameraFrame {
    pub motion_index: i32,
    pub motion_offset: usize,
    pub attributes_offset: usize,
    pub other_motion_offset: usize,
    pub frame: f32,
    pub frame_count: u32,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub roll_bams: i32,
    pub fov_bams: i32,
    pub attributes: CameraAttributes,
}

fn span(bank: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    bank.get(
        at..at
            .checked_add(size)
            .ok_or_else(|| Error::invalid("camera offset overflow"))?,
    )
    .ok_or_else(|| Error::invalid("truncated camera data"))
}
fn word(s: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(s[at..at + 4].try_into().unwrap())
}
fn real(s: &[u8], at: usize) -> Result<f32> {
    let v = f32::from_bits(word(s, at));
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Error::invalid("nonfinite camera value"))
    }
}
fn vec3(s: &[u8], at: usize) -> Result<[f32; 3]> {
    Ok([real(s, at)?, real(s, at + 4)?, real(s, at + 8)?])
}

struct Segment<'a> {
    a: &'a [u8],
    b: &'a [u8],
    dist: f32,
    delta: f32,
}

fn segment<'a>(
    bank: &'a [u8],
    descriptor: &[u8],
    channel: usize,
    stride: usize,
    frame: f32,
    frames: u32,
) -> Result<Segment<'a>> {
    let at = word(descriptor, channel * 4) as usize;
    let n = word(descriptor, 16 + channel * 4) as usize;
    if at == 0 || n == 0 || n > frames as usize {
        return Err(Error::invalid("empty or excessive camera keys"));
    }
    let keys = span(bank, at, n * stride)?;
    let mut previous = None;
    let mut next = None;
    let mut last = None;
    for k in keys.chunks_exact(stride) {
        let t = word(k, 0);
        if t >= frames || last.is_some_and(|v| t <= v) {
            return Err(Error::invalid(
                "camera key times are unordered or outside clip",
            ));
        }
        if t as f32 <= frame {
            previous = Some(k);
        } else if next.is_none() {
            next = Some(k);
        }
        last = Some(t);
    }
    if word(keys, 0) != 0 {
        return Err(Error::invalid(
            "unsupported camera channel without a first-frame key",
        ));
    }
    let a = previous.unwrap();
    let (b, t) = next.map_or((&keys[..stride], frames), |k| (k, word(k, 0)));
    let start = word(a, 0);
    Ok(Segment {
        a,
        b,
        dist: (t - start) as f32,
        delta: frame - start as f32,
    })
}
fn vector(s: Segment<'_>) -> Result<[f32; 3]> {
    let a = vec3(s.a, 4)?;
    if s.delta == 0.0 {
        return Ok(a);
    }
    let b = vec3(s.b, 4)?;
    let out = std::array::from_fn(|i| (b[i] - a[i]) / s.dist * s.delta + a[i]);
    if out.iter().all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err(Error::invalid("camera interpolation overflow"))
    }
}
fn angle(s: Segment<'_>) -> i32 {
    let a = word(s.a, 4) as i32;
    if s.delta == 0.0 {
        return a;
    }
    let b = word(s.b, 4) as i32;
    (f64::from(b.wrapping_sub(a)) / f64::from(s.dist) * f64::from(s.delta) + f64::from(a)).round()
        as i32
}

/// Sample one explicitly selected camera. Frame time is in the camera motion's
/// native frames. A final fractional frame interpolates toward frame zero.
pub fn sample(bank: &[u8], motion_index: i32, frame: f32) -> Result<CameraFrame> {
    if bank.len() > 64 * 1024 * 1024
        || !(0..65536).contains(&motion_index)
        || !frame.is_finite()
        || frame < 0.0
    {
        return Err(Error::invalid(
            "invalid camera selection or motion bank size",
        ));
    }
    let count = bank
        .as_chunks::<8>()
        .0
        .iter()
        .take(65537)
        .position(|row| *row == [0; 8])
        .ok_or_else(|| Error::invalid("unterminated camera motion directory"))?;
    if motion_index as usize >= count {
        return Err(Error::invalid("camera index outside motion directory"));
    }
    let e = span(bank, motion_index as usize * 8, 8)?;
    if word(e, 4) != 1 || word(e, 0) == u32::MAX {
        return Err(Error::invalid("selected motion is not a one-part camera"));
    }
    let motion_offset = word(e, 0) as usize;
    let m = span(bank, motion_offset, 20)?;
    if word(m, 8) != 0x01c10004 {
        return Err(Error::invalid(
            "unsupported camera motion flags or interpolation",
        ));
    }
    let frame_count = word(m, 4);
    if frame_count == 0 || frame_count > 1_000_000 || frame >= frame_count as f32 {
        return Err(Error::invalid("frame outside camera motion duration"));
    }
    let attributes_offset = word(m, 12) as usize;
    let other_motion_offset = word(m, 16) as usize;
    if word(m, 0) == 0 || attributes_offset == 0 || other_motion_offset == 0 {
        return Err(Error::invalid("null camera reference"));
    }
    span(bank, other_motion_offset, 12)?;
    let a = span(bank, attributes_offset, 64)?;
    let attributes = CameraAttributes {
        position: vec3(a, 0)?,
        direction_z: vec3(a, 12)?,
        roll_bams: word(a, 24) as i32,
        fov_bams: word(a, 28) as i32,
        near_clip: real(a, 32)?,
        far_clip: real(a, 36)?,
        direction_x: vec3(a, 40)?,
        direction_y: vec3(a, 52)?,
    };
    let d = span(bank, word(m, 0) as usize, 32)?;
    Ok(CameraFrame {
        motion_index,
        motion_offset,
        attributes_offset,
        other_motion_offset,
        frame,
        frame_count,
        position: vector(segment(bank, d, 0, 16, frame, frame_count)?)?,
        target: vector(segment(bank, d, 1, 16, frame, frame_count)?)?,
        roll_bams: angle(segment(bank, d, 2, 8, frame, frame_count)?),
        fov_bams: angle(segment(bank, d, 3, 8, frame, frame_count)?),
        attributes,
    })
}
