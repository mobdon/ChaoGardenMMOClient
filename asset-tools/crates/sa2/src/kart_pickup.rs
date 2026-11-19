//! Single kart ring task at CartD +0xa824. Task keys identify collision partners.
use crate::{city_rel::CityRel, kart_progress, Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    mode: i8,
    timer: u16,
    yaw: i32,
    scale_z: f32,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Kart {
    task: u32,
    work_44: i32,
    work_70: i32,
    work_d8: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    state: State,
    first_range_metric: f32,
    view_count: i32,
    second_range_metric: Option<f32>,
    registry: [Option<Kart>; 6],
    collision_flags: u16,
    contacts: Vec<u32>,
    has_entry: bool,
    collection: Option<CollectionInput>,
}
#[derive(Serialize)]
pub struct Output {
    state: State,
    registry: [Option<Kart>; 6],
    calls: Vec<Call>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection: Option<Vec<Spawn>>,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Call {
    InstallCallbacks([u32; 2]),
    InitCollision {
        section5_offset: u32,
        args: [i32; 2],
    },
    RegisterCollision,
    Engine80063584([i32; 2]),
    Engine8006b710([i32; 4]),
    CollectionEffectAb04,
    Engine80065148,
    Engine80071888,
}
pub fn tick(bytes: &[u8], s: Input) -> Result<Output> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let c = rel.data(4)?;
    let f = |at: usize| f32::from_be_bytes(c[at..at + 4].try_into().unwrap());
    let outside = crate::kart_object_range::ObjectRange {
        first: s.first_range_metric,
        views: s.view_count,
        second: s.second_range_metric,
    }
    .outside()?;
    let mut o = Output {
        state: s.state,
        registry: s.registry,
        calls: vec![],
        collection: None,
    };
    if outside && o.state.mode != 4 {
        return Ok(o);
    }
    match o.state.mode {
        0 => {
            o.calls.push(Call::InstallCallbacks([0xaa64, 0xab00]));
            o.calls.push(Call::InitCollision {
                section5_offset: 0x2ed3e8,
                args: [1, 5],
            });
            o.state.mode = 1;
        }
        1 => {
            o.state.scale_z = f(0x3b0);
            o.state.mode = 2;
            o.calls.push(Call::RegisterCollision);
        }
        2 => {
            o.state.scale_z = (o.state.scale_z + f(0x3b4)).min(f(0x3b8));
            if s.collision_flags & 1 != 0 {
                if s.contacts.len() > i16::MAX as usize {
                    return Err(Error::invalid("collision list exceeds native signed count"));
                }
                for task in s.contacts {
                    if task == 0 {
                        continue;
                    }
                    if let Some(k) = o.registry.iter_mut().flatten().find(|k| k.task == task) {
                        o.state.mode = 3;
                        if k.work_44 != 0 {
                            o.calls.push(Call::Engine80063584([k.work_70, 1]));
                            o.calls.push(Call::Engine8006b710([0x8005, 0, 0, 0]));
                            k.work_d8 = k.work_d8.wrapping_add(1);
                        }
                    }
                }
            }
            o.calls.push(Call::RegisterCollision);
        }
        3 => {
            o.calls.push(Call::CollectionEffectAb04);
            o.collection = s.collection.map(|input| collection(c, input)).transpose()?;
            o.state.mode = 4;
            o.state.timer = 3600;
        }
        4 => {
            o.state.timer = o.state.timer.wrapping_sub(1);
            if o.state.timer == 0 {
                o.state.mode = 1;
            }
        }
        5 => {
            o.calls.push(if s.has_entry {
                Call::Engine80065148
            } else {
                Call::Engine80071888
            });
            return Ok(o);
        }
        _ => {}
    }
    o.state.yaw = o.state.yaw.wrapping_add(364);
    Ok(o)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionInput {
    position: [f32; 3],
    // Results of the two imported trig calls, in their original call order.
    trig_xz: [[f32; 2]; 4],
    // A null allocation consumes no RNG. Each successful one consumes three draws.
    allocation_draws: [Option<[i32; 3]>; 4],
}
#[derive(Serialize)]
struct Spawn {
    angle: i32,
    position: [f32; 3],
    velocity: [f32; 3],
    scale: f32,
    writes: Option<ParticleWrites>,
}
#[derive(Serialize)]
struct ParticleWrites {
    word_00: i32,
    float_18: f32,
    word_2c: i32,
    word_38: u32,
}
/// Four calls to 0xf5e8, in order; retain only fields the constructor initializes.
fn collection(data: &[u8], s: CollectionInput) -> Result<Vec<Spawn>> {
    let f = |at: usize| f32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    let trunc = |v: f32| {
        if (-2147483648.0..2147483648.0).contains(&v) {
            Ok(v as i32)
        } else {
            Err(Error::invalid(
                "particle angle outside supported fctiwz range",
            ))
        }
    };
    let mut out = Vec::with_capacity(4);
    for (i, ([x, z], draws)) in s.trig_xz.into_iter().zip(s.allocation_draws).enumerate() {
        let position = [
            s.position[0] + f(0x3c0) * x,
            (s.position[1] + f(0x3c4) * (4 - i) as f32) - f(0x3c8),
            s.position[2] + f(0x3c0) * z,
        ];
        if !position.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid("nonfinite kart ring particle position"));
        }
        let writes = draws
            .map(|[a, b, c]| -> Result<_> {
                let unit = |v: i32| f(0x4e4) * v as f32;
                Ok(ParticleWrites {
                    word_00: trunc(f(0x4e0) * (f(0x4f8) * unit(a) - f(0x508)))?,
                    word_2c: trunc(f(0x4e0) * (unit(b) - f(0x4e8)))?,
                    float_18: f(0x4e8) * unit(c),
                    word_38: 0,
                })
            })
            .transpose()?;
        out.push(Spawn {
            angle: (i as i32) * 0x4000,
            position,
            velocity: [f(0x3b0), f(0x3bc), f(0x3b0)],
            scale: f(0x3cc),
            writes,
        });
    }
    Ok(out)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Particle {
    angle: i32,
    scale: f32,
    position: [f32; 3],
    color: u32,
    frame: f32,
    orientation: i32,
    velocity: [f32; 3],
    spin: i32,
    phase: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleInput {
    particle: Particle,
    phase_sine: f32,
}
#[derive(Serialize)]
pub struct ParticleUpdate {
    particle: Particle,
    alive: bool,
}
/// Update referenced by the native ring effect descriptor at +0x2ee6d0.
pub fn particle_tick(bytes: &[u8], s: ParticleInput) -> Result<ParticleUpdate> {
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    if rel.local_pointer(5, 0x2ee6f0)? != Some((1, 0xf784)) {
        return Err(Error::invalid("unexpected kart ring effect update"));
    }
    let d = &rel.data(5)?[0x2ee6d0..];
    let n = u32::from_be_bytes(d[12..16].try_into().unwrap());
    let f = |i: usize| f32::from_be_bytes(d[i..i + 4].try_into().unwrap());
    let c = rel.data(4)?;
    let cf = |i: usize| f32::from_be_bytes(c[i..i + 4].try_into().unwrap());
    let mut p = s.particle;
    p.angle = p.angle.wrapping_add(p.spin);
    p.scale += f(28);
    for i in 0..3 {
        p.position[i] += p.velocity[i];
        p.velocity[i] *= f(20);
    }
    p.velocity[1] += f(24);
    p.frame += f(16);
    let angle = cf(0x4e0) * (cf(0x4f8) * s.phase_sine);
    if !(-2147483648.0..2147483648.0).contains(&angle)
        || !(-2147483648.0..2147483648.0).contains(&p.frame)
    {
        return Err(Error::invalid("unsupported particle conversion"));
    }
    p.orientation = (angle as i32).wrapping_add(0x4000);
    p.phase = p.phase.wrapping_add(0x222);
    let mut alive = true;
    if (p.frame as i32 as i16 as i32 as u32) >= n {
        p.frame = n.wrapping_sub(1) as f32;
        p.color = (p.color & 0x00ffffff) | ((p.color >> 1) & 0xff000000);
        alive = p.color >> 24 != 0;
    }
    Ok(ParticleUpdate { particle: p, alive })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    mode: i8,
    position: [f32; 3],
    yaw: i32,
    scale_z: f32,
    current_view_metric: f32,
}
/// Display is gated by the active view only (0x17e8); the update checks both views.
pub fn display(bytes: &[u8], pak: &[u8], s: DisplayInput) -> Result<serde_json::Value> {
    use serde_json::json;
    kart_progress::verify(bytes)?;
    let rel = CityRel::read_identified(bytes, 80)?;
    let threshold = f32::from_be_bytes(rel.data(4)?[0x1c0..0x1c4].try_into().unwrap());
    if s.current_view_metric >= threshold || s.mode != 2 {
        return Ok(json!({"draw":null}));
    }
    let invalid = || Error::invalid("unsupported kart ring display reference");
    let target = |site| {
        rel.reference(1, site)
            .filter(|r| r.kind == 4 && r.module == 80 && r.section == 5)
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let ptr = |site| {
        rel.local_pointer(5, site)?
            .filter(|(sec, _)| *sec == 5)
            .map(|(_, o)| o)
            .ok_or_else(invalid)
    };
    let list = target(0xaa9e)?;
    let attach = target(0xaae2)?;
    let d = rel.data(5)?;
    if d[list as usize + 4..list as usize + 8] != 1u32.to_be_bytes() {
        return Err(invalid());
    }
    let at = ptr(ptr(list)?)? as usize;
    let name = std::ffi::CStr::from_bytes_until_nul(&d[at..])
        .map_err(|_| invalid())?
        .to_str()
        .map_err(|_| invalid())?;
    let member = format!("objtex_common\\{name}.dds");
    let bank = crate::archive::read_pak(pak)?;
    let mut matches = bank.iter().filter(|e| e.name.eq_ignore_ascii_case(&member));
    let entry = matches.next().ok_or_else(invalid)?;
    if matches.next().is_some() {
        return Err(invalid());
    }
    let image = crate::texture::decode_dds(entry.data)?;
    let geometry =
        crate::kart_story_target::vertices(&rel, attach, &mut crate::visual::Material::default())?;
    Ok(
        json!({"draw":{"textureList":list,"attach":attach,"position":s.position,"yaw":s.yaw,"scale":([s.scale_z;3]),
        "geometry":geometry,"texture":{"name":name,"entry":entry.name,"offset":entry.offset,"length":entry.length,
        "ddsSha256":crate::manifest::sha256(entry.data),"width":image.width,"height":image.height,
        "rgbaSha256":crate::manifest::sha256(&image.rgba),"nonopaquePixels":image.rgba.chunks_exact(4).filter(|p|p[3]!=255).count()}}}),
    )
}
