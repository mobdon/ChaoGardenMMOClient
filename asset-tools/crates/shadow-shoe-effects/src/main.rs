use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, fs, io::Write, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LIMIT: usize = 16 * 1024 * 1024;
const HASHES: [&str; 8] = [
    "3165d15ef6e2322dd3cdbe3098eef5366129094b8e9d2767d464b050f628e96e",
    "b6f4cbe6703bbc15d654cb3ae68cd3c808ae6fa20848c67c0be25b3989268970",
    "b0207fa753b92932ddbc0ca35e57aa5231341e354eb0edf78726605c8306567d",
    "3dfd982639e01f1361c9c4b60af0a49f702f6c7809c9d141f8a075b62d2c1bbb",
    "ae93732933aefb648d1f4ca53bb99c7b90ab8c6e7aefaa57d783a562a31840b3",
    "3bab1f2ee36125dc1e52a3e827f8c430cc77f227fa28a0e729279061eb915d14",
    "a7a34115e1e9cc3900e0a61e21c6d7325374118dd832cb4341d7567f72dcaad3",
    "8d2c3955582ca8c49e48f7ff8bab6b50ffa892d7bfa19933de8305cff02e76dd",
];
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > LIMIT as u64 {
        return Err("input exceeds16MiB".into());
    }
    Ok(fs::read(path)?)
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let end = at.checked_add(4).ok_or("invalid word offset")?;
    Ok(u32::from_le_bytes(
        bytes.get(at..end).ok_or("truncated word")?.try_into()?,
    ))
}
fn unpack(bytes: &[u8]) -> Result<(Value, Vec<u8>)> {
    if u32_at(bytes, 0)? != 0x46546c67
        || u32_at(bytes, 4)? != 2
        || u32_at(bytes, 8)? as usize != bytes.len()
    {
        return Err("invalid GLB header".into());
    }
    let n = u32_at(bytes, 12)? as usize;
    if u32_at(bytes, 16)? != 0x4e4f534a {
        return Err("expected JSON chunk".into());
    }
    let document: Value = serde_json::from_slice(bytes.get(20..20 + n).ok_or("truncated JSON")?)?;
    if document["buffers"]
        .as_array()
        .ok_or("missing buffer")?
        .len()
        != 1
        || !document["buffers"][0].is_object()
        || !document["buffers"][0]["uri"].is_null()
    {
        return Err("only one embedded GLB buffer supported".into());
    }
    if u32_at(bytes, 24 + n)? != 0x004e4942 {
        return Err("expected BIN chunk".into());
    }
    let size = u32_at(bytes, 20 + n)? as usize;
    if 28 + n + size != bytes.len() {
        return Err("unexpected trailing GLB data".into());
    }
    Ok((document, bytes[28 + n..].to_vec()))
}
fn pack(document: &Value, binary: &[u8]) -> Result<Vec<u8>> {
    let mut text = serde_json::to_vec(document)?;
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let mut bytes = Vec::new();
    for word in [
        0x46546c67,
        2,
        (28 + text.len() + binary.len()) as u32,
        text.len() as u32,
        0x4e4f534a,
    ] {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(text);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(0x004e4942u32.to_le_bytes());
    bytes.extend(binary);
    Ok(bytes)
}
fn push(document: &mut Value, key: &str, value: Value) -> usize {
    if document[key].is_null() {
        document[key] = json!([]);
    }
    let rows = document[key].as_array_mut().expect("validated glTF array");
    let index = rows.len();
    rows.push(value);
    index
}
fn accessor(
    document: &mut Value,
    binary: &mut Vec<u8>,
    data: &[u8],
    component: u32,
    count: usize,
    kind: &str,
) -> usize {
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let view = push(
        document,
        "bufferViews",
        json!({"buffer":0,"byteOffset":binary.len(),"byteLength":data.len()}),
    );
    binary.extend(data);
    push(
        document,
        "accessors",
        json!({"bufferView":view,"componentType":component,"count":count,"type":kind}),
    )
}
fn strips(words: &[u16], vertex_count: usize) -> Result<Vec<u16>> {
    if words.len() < 4 || words[0] & 255 != 64 || !(2..=3).contains(&words[2]) {
        return Err("unsupported native strip format".into());
    }
    let mut at = 3;
    let mut triangles = Vec::new();
    for _ in 0..words[2] {
        let count = *words.get(at).ok_or("truncated strip")? as i16;
        at += 1;
        let size = count.unsigned_abs() as usize;
        if !(3..=64).contains(&size) {
            return Err("invalid strip length".into());
        }
        let strip = words.get(at..at + size).ok_or("truncated strip indices")?;
        at += size;
        if strip.iter().any(|&v| v as usize >= vertex_count) {
            return Err("strip index out of bounds".into());
        }
        for i in 0..size - 2 {
            let (a, b) = if (i % 2 == 0) == (count > 0) {
                (strip[i], strip[i + 1])
            } else {
                (strip[i + 1], strip[i])
            };
            triangles.extend([a, b, strip[i + 2]]);
        }
    }
    if (at != words.len() && !(at + 1 == words.len() && words[at] == 0)) || triangles.len() != 48 {
        return Err("unexpected native jet topology".into());
    }
    Ok(triangles)
}
fn linear(value: u8) -> f32 {
    let s = value as f32 / 255.;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}
fn bundle(
    side: &str,
    boot: &Path,
    upgrade: &Path,
    native_dir: &Path,
    inspected_dir: &Path,
    output: &Path,
) -> Result<()> {
    let ids = match side {
        "left" => 92..96,
        "right" => 96..100,
        _ => return Err("side must be left/right".into()),
    };
    let local = env::current_dir()?.join(".local").canonicalize()?;
    if output.exists()
        || !output
            .parent()
            .ok_or("missing output parent")?
            .canonicalize()?
            .starts_with(local)
    {
        return Err("output must be new and under this checkout's .local".into());
    }
    let provenance = output.with_extension("glb.provenance.json");
    if provenance.exists() {
        return Err("provenance already exists".into());
    }
    let boot_bytes = read(boot)?;
    let (mut doc, mut binary) = unpack(&boot_bytes)?;
    let original_binary_sha = sha(&binary);
    for key in [
        "nodes",
        "meshes",
        "materials",
        "accessors",
        "bufferViews",
        "scenes",
    ] {
        if !doc[key].is_array() {
            return Err(format!("missing glTF {key}").into());
        }
    }
    let report: Value = serde_json::from_slice(&read(upgrade)?)?;
    let record = report["assets"]
        .as_array()
        .ok_or("invalid upgrade report")?
        .iter()
        .find(|row| row["name"] == format!("shadow-air-shoes-{side}"))
        .ok_or("missing native shoe report")?;
    if record["sha256"].as_str() != Some(&sha(&boot_bytes)) {
        return Err("original shoe hash mismatch".into());
    }
    let center = record["removedBlenderCenter"]
        .as_array()
        .ok_or("missing shoe recentering")?;
    if center.len() != 3 {
        return Err("invalid recentering".into());
    }
    let value = |i: usize| center[i].as_f64().ok_or("invalid center");
    let shift = [value(0)? as f32, value(2)? as f32, -value(1)? as f32];
    let mut sources = Vec::new();
    for id in ids {
        let raw = read(&native_dir.join(format!("{id}.sa2mdl")))?;
        if sha(&raw) != HASHES[id - 92] {
            return Err(format!("native model{id} fingerprint mismatch").into());
        }
        let decoded_path = inspected_dir.join(format!("model{id}.json"));
        let decoded = read(&decoded_path)?;
        let manifest: Value =
            serde_json::from_slice(&read(&decoded_path.with_extension("json.manifest.json"))?)?;
        if manifest["sources"][0]["sha256"] != HASHES[id - 92]
            || manifest["assets"][0]["output"]["sha256"] != sha(&decoded)
        {
            return Err("native Rust inspector provenance mismatch".into());
        }
        let native: Value = serde_json::from_slice(&decoded)?;
        if native["nodes"].as_array().map(Vec::len) != Some(1)
            || native["nodes"][0]["position"] != json!([0., 0., 0.])
            || native["nodes"][0]["rotationBams"] != json!([0, 0, 0])
            || native["nodes"][0]["scale"] != json!([1., 1., 1.])
        {
            return Err("unsupported native effect transform".into());
        }
        let attach = &native["attachments"][0];
        let chunks = &attach["vertexChunks"];
        if chunks.as_array().map(Vec::len) != Some(1) || chunks[0]["kind"] != 35 {
            return Err("unsupported jet vertices".into());
        }
        let vertices = chunks[0]["vertices"]
            .as_array()
            .ok_or("missing jet vertices")?;
        if vertices.len() != 12 {
            return Err("unexpected jet vertex count".into());
        }
        let vertex_offset = usize::try_from(
            chunks[0]["offset"]
                .as_u64()
                .ok_or("missing native vertex offset")?,
        )?;
        let vertex_bytes = raw
            .get(vertex_offset..)
            .ok_or("invalid native vertex offset")?;
        for (index, vertex) in vertices.iter().enumerate() {
            for word in 0..4 {
                let actual = u32_at(vertex_bytes, 8 + index * 16 + word * 4)?;
                if vertex["rawWords"][word].as_u64() != Some(actual as u64)
                    || word < 3
                        && (vertex["position"][word]
                            .as_f64()
                            .ok_or("missing native coordinate")? as f32)
                            .to_bits()
                            != actual
                {
                    return Err("native vertex bytes differ from inspected data".into());
                }
            }
        }
        let polygons = attach["polygonChunks"]
            .as_array()
            .ok_or("missing polygon chunks")?;
        if polygons.len() != 2
            || polygons[0]["kind"] != 19
            || polygons[0]["flags"] != 37
            || polygons[0]["diffuseRgba"] != json!([178, 178, 178, 255])
            || polygons.iter().any(|p| !p["textureId"].is_null())
        {
            return Err("unverified native material state".into());
        }
        for polygon in polygons {
            let offset = usize::try_from(
                polygon["offset"]
                    .as_u64()
                    .ok_or("missing native polygon offset")?,
            )?;
            let polygon_bytes = raw.get(offset..).ok_or("invalid native polygon offset")?;
            for (index, word) in polygon["rawWords"]
                .as_array()
                .ok_or("missing native polygon words")?
                .iter()
                .enumerate()
            {
                let bytes: [u8; 2] = polygon_bytes
                    .get(index * 2..index * 2 + 2)
                    .ok_or("truncated native polygon")?
                    .try_into()?;
                if word.as_u64() != Some(u16::from_le_bytes(bytes) as u64) {
                    return Err("native polygon bytes differ from inspected data".into());
                }
            }
        }
        let words = polygons[1]["rawWords"]
            .as_array()
            .ok_or("missing strips")?
            .iter()
            .map(|w| {
                w.as_u64()
                    .and_then(|v| u16::try_from(v).ok())
                    .ok_or("bad strip word")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let triangles = strips(&words, vertices.len())?;
        let mut positions = Vec::new();
        let mut colors = Vec::new();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in vertices {
            for axis in 0..3 {
                let position = vertex["position"][axis]
                    .as_f64()
                    .ok_or("missing position")? as f32
                    - shift[axis];
                if !position.is_finite() {
                    return Err("nonfinite vertex".into());
                }
                min[axis] = min[axis].min(position);
                max[axis] = max[axis].max(position);
                positions.extend(position.to_le_bytes());
            }
            let argb = vertex["rawWords"][3].as_u64().ok_or("missing ARGB")? as u32;
            if argb != 0xffffffff && argb != 0x00ff8f20 {
                return Err("unexpected native vertex color".into());
            }
            for component in [
                linear((argb >> 16) as u8),
                linear((argb >> 8) as u8),
                linear(argb as u8),
                (argb >> 24) as f32 / 255.,
            ] {
                colors.extend(component.to_le_bytes());
            }
        }
        let position = accessor(&mut doc, &mut binary, &positions, 5126, 12, "VEC3");
        doc["accessors"][position]["min"] = json!(min);
        doc["accessors"][position]["max"] = json!(max);
        let color = accessor(&mut doc, &mut binary, &colors, 5126, 12, "VEC4");
        let indices = triangles
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let index = accessor(&mut doc, &mut binary, &indices, 5123, 48, "SCALAR");
        let (part, variant) = match id % 4 {
            0 => ("toe", "narrow"),
            1 => ("toe", "wide"),
            2 => ("heel", "wide"),
            _ => ("heel", "narrow"),
        };
        let metadata = json!({"modelId":id,"side":side,"part":part,"variant":variant,"nativeSha256":HASHES[id-92],"nativeBlend":"SrcAlpha/InvSrcAlpha","useTexture":false,"vertexColor":"originalARGB; RGB converted to linear as SAIO2.2.2"});
        let mat = push(
            &mut doc,
            "materials",
            json!({"name":format!("native-shadow-jet-{id}"),"alphaMode":"BLEND","doubleSided":true,
            "pbrMetallicRoughness":{"baseColorFactor":[178./255.,178./255.,178./255.,1.],"metallicFactor":0.,"roughnessFactor":1.},"extensions":{"KHR_materials_unlit":{}},
            "extras":{"sa2Source":{"ignoreLighting":true,"useAlpha":true,"sourceAlpha":"SRC","destinationAlpha":"INV_SRC","useTexture":false},"sa2ShoeEffect":metadata}}),
        );
        let name = format!("native-shadow-jet-{id}-{side}-{part}-{variant}");
        let mesh = push(
            &mut doc,
            "meshes",
            json!({"name":name,"primitives":[{"attributes":{"POSITION":position,"COLOR_0":color},"indices":index,"material":mat,"mode":4}],"extras":{"sa2ShoeEffect":metadata}}),
        );
        let node = push(
            &mut doc,
            "nodes",
            json!({"name":name,"mesh":mesh,"extras":{"sa2ShoeEffect":metadata}}),
        );
        let scene = usize::try_from(doc["scene"].as_u64().unwrap_or(0))?;
        doc.get_mut("scenes")
            .and_then(Value::as_array_mut)
            .and_then(|scenes| scenes.get_mut(scene))
            .and_then(|scene| scene.get_mut("nodes"))
            .and_then(Value::as_array_mut)
            .ok_or("missing scene roots")?
            .push(json!(node));
        sources.push(json!({"modelId":id,"sha256":HASHES[id-92],"inspectionSha256":sha(&decoded),"vertices":12,"triangles":16,"part":part,"variant":variant}));
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    doc["buffers"][0]["byteLength"] = json!(binary.len());
    if doc["extensionsUsed"].is_null() {
        doc["extensionsUsed"] = json!([]);
    }
    if !doc["extensionsUsed"]
        .as_array()
        .ok_or("invalid extensions")?
        .contains(&json!("KHR_materials_unlit"))
    {
        doc["extensionsUsed"]
            .as_array_mut()
            .unwrap()
            .push(json!("KHR_materials_unlit"));
    }
    let result = pack(&doc, &binary)?;
    let proof = json!({"version":1,"side":side,"originalShoeSha256":sha(&boot_bytes),"originalBinarySha256":original_binary_sha,"originalBinaryBytes":unpack(&boot_bytes)?.1.len(),"outputSha256":sha(&result),"shoeOriginShift":shift,"sources":sources,
        "limits":["Original shoe buffers/images/accessors preserved; native jets recentered by same source shoe offset","Eligibility and narrow/wide selection are presentation adaptations; native effect draw-time matrices/timing are not traced","No installed-game file or live runtime is modified; prepared aliases require reviewed installation"]});
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?
        .write_all(&result)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(provenance)?
        .write_all(&serde_json::to_vec_pretty(&proof)?)?;
    println!("Prepared {side} native shoe jets: {}", sha(&result));
    Ok(())
}
fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 7 {
        return Err("usage: shadow-shoe-effects bundle SIDE BOOT.glb UPGRADE_REPORT.json NATIVE_MODEL_DIR RUST_INSPECTION_DIR OUTPUT.glb".into());
    }
    if args[0] != "bundle" {
        return Err("expected bundle command".into());
    }
    bundle(
        &args[1],
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
        Path::new(&args[5]),
        Path::new(&args[6]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_strips_preserve_signed_winding_and_bounds() {
        let words = [
            39744, 26, 3, 6, 8, 7, 6, 5, 4, 3, 6, 2, 1, 0, 11, 10, 9, 65526, 4, 2, 3, 1, 5, 11, 7,
            9, 8, 10,
        ];
        let indices = strips(&words, 12).unwrap();
        assert_eq!(indices.len(), 48);
        assert_eq!(&indices[..6], &[8, 7, 6, 6, 7, 5]);
        assert_eq!(&indices[24..30], &[2, 4, 3, 2, 3, 1]);
        assert!(strips(&words, 11).is_err());
        assert!(strips(&words[..20], 12).is_err());
    }
    #[test]
    fn embedded_glb_roundtrip_retains_original_bytes() {
        let document = json!({"buffers":[{"byteLength":4}]});
        let bin = [1, 2, 3, 4];
        let bytes = pack(&document, &bin).unwrap();
        let (got, data) = unpack(&bytes).unwrap();
        assert_eq!(got, document);
        assert_eq!(data, bin);
        assert!(unpack(&bytes[..bytes.len() - 1]).is_err());
    }
    #[test]
    fn native_color_alpha_is_preserved_and_rgb_matches_srgb_conversion() {
        assert_eq!(linear(255), 1.);
        assert_eq!(linear(0), 0.);
        assert!((linear(143) - 0.2746773).abs() < 1e-6);
        assert!((linear(32) - 0.01444384).abs() < 1e-6);
    }
}
