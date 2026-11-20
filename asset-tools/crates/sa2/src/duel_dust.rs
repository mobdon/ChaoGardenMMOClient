//! The stage 42 module's rectangle dust helper (stg42D.rel code +0x1E8DC)
//! and its particle spawner (+0x1DDFC). DODAI calls it as a platform starts
//! to fall. Points along the four edges of a width × depth rectangle are
//! transformed by the engine; each gets one particle moving outward. The
//! engine transform, vector length, allocation and rand results are inputs.
use crate::duel_platform;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DustConstants {
    pub spacing_numerator: f32,
    pub count_scale: f32,
    pub zero: f32,
    pub descriptor: u32,
    pub rand_scale: f32,
    pub yaw_range: f32,
    pub yaw_offset: f32,
    pub bams: f32,
    pub roll_offset: f32,
    pub size_range: f32,
}
pub fn constants(rel_bytes: &[u8]) -> Result<DustConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    Ok(DustConstants {
        spacing_numerator: f(0x1e95e)?,
        count_scale: f(0x1e96e)?,
        zero: f(0x1e9a2)?,
        descriptor: duel_platform::target(&rel, 0x1de2e, 5)?,
        rand_scale: f(0x1dea2)?,
        yaw_range: f(0x1deb2)?,
        yaw_offset: f(0x1dec2)?,
        bams: f(0x1ded2)?,
        roll_offset: f(0x1df1e)?,
        size_range: f(0x1df72)?,
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EdgeTransform {
    /// njCalcPoint result (0x80114f0c).
    pub point: [f32; 3],
    /// njCalcVector result (0x801154ec).
    pub vector: [f32; 3],
    /// njScalor of that vector (0x801151f4).
    pub length: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DustCall {
    pub position: [f32; 3],
    pub angles: [i32; 3],
    /// f1..f5: half width, half depth, particle size, density, speed.
    pub args: [f32; 5],
    /// One per edge point, in the order the helper visits them.
    pub transforms: Vec<EdgeTransform>,
    /// Indices of particle allocations that return null.
    pub failed_allocations: Vec<u32>,
    pub rand: Vec<i32>,
}

fn engine(address: u32, args: Value) -> Value {
    json!({ "engine": format!("0x{address:08x}"), "args": args })
}

/// Run the helper once and return its ordered engine calls and particle writes.
pub fn run(c: &DustConstants, a: &DustCall) -> Result<Vec<Value>> {
    let [width, depth, size, density, speed] = a.args;
    let spacing = c.spacing_numerator / density;
    let nx = (c.count_scale * width / spacing) as i32;
    let nz = (c.count_scale * depth / spacing) as i32;
    let mut calls = vec![
        engine(0x80116098, json!([])),
        engine(0x801153e4, json!([0])),
        engine(0x8011616c, json!({ "translate": a.position })),
        engine(0x80115828, json!([0, a.angles[1]])),
        engine(0x801158a8, json!([0, a.angles[2]])),
    ];
    let mut points = a.transforms.iter();
    let mut rand = a.rand.iter().copied();
    let mut alloc = 0u32;
    let mut emit = |local: [f32; 3], calls: &mut Vec<Value>| -> Result<()> {
        let t = points
            .next()
            .ok_or_else(|| Error::invalid("dust transforms exhausted"))?;
        calls.push(engine(
            0x80114f0c,
            json!({ "point": local, "returned": t.point }),
        ));
        calls.push(engine(
            0x801154ec,
            json!({ "vector": local, "returned": t.vector }),
        ));
        calls.push(engine(
            0x801151f4,
            json!({ "vector": t.vector, "returned": t.length }),
        ));
        let k = speed / t.length;
        let v = t.vector.map(|x| x * k);
        let made = !a.failed_allocations.contains(&alloc);
        alloc += 1;
        calls.push(engine(
            0x80032b78,
            json!({ "descriptor": format!("s5+0x{:x}", c.descriptor), "returnedNonzero": made }),
        ));
        if made {
            let mut draw = |calls: &mut Vec<Value>| -> Result<f32> {
                let r = rand
                    .next()
                    .ok_or_else(|| Error::invalid("dust rand values exhausted"))?;
                calls.push(engine(0x8010dc88, json!({ "returned": r })));
                Ok(c.rand_scale * r as f64 as f32)
            };
            let yaw = (c.bams * (c.yaw_range * draw(calls)? - c.yaw_offset)) as i32;
            let roll = (c.bams * (draw(calls)? - c.roll_offset)) as i32;
            let span = c.size_range * draw(calls)?;
            let add = draw(calls)?;
            calls.push(
                json!({ "particle": { "position": t.point, "velocity": v, "size": size,
                "angle0": yaw, "angle2C": roll, "field38": span, "add18": add } }),
            );
        }
        Ok(())
    };
    for i in 0..nx {
        let x = i as f64 as f32 * spacing - width;
        emit([x, c.zero, depth], &mut calls)?;
        emit([x, c.zero, -depth], &mut calls)?;
    }
    for j in 0..nz {
        let z = j as f64 as f32 * spacing - depth;
        emit([width, c.zero, z], &mut calls)?;
        emit([-width, c.zero, z], &mut calls)?;
    }
    calls.push(engine(0x801160e4, json!([])));
    Ok(calls)
}

pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let c = constants(rel_bytes)?;
    let a: DustCall = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("dust request: {e}")))?;
    Ok(json!({ "constants": c, "calls": run(&c, &a)? }))
}
