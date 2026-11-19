use sa2_asset_tools::{
    manifest::sha256, model::read_model, visual::evaluate_static_model, visual_glb::export,
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

fn section(doc: &Value, view: usize, bin: &[u8]) -> Vec<u8> {
    let v = &doc["bufferViews"][view];
    let offset = v["byteOffset"].as_u64().unwrap() as usize;
    assert_eq!(offset % 4, 0);
    bin[offset..offset + v["byteLength"].as_u64().unwrap() as usize].to_vec()
}

#[test]
#[ignore = "requires local original sources and extracted original texture PNGs"]
fn export_and_read_all_static_glbs() {
    let local = PathBuf::from(std::env::var_os("SA2_VISUAL_LOCAL").unwrap());
    for (i, name) in [
        "blue",
        "green",
        "lightblue",
        "purple",
        "red",
        "white",
        "yellow",
        "tree-neutral",
    ]
    .iter()
    .enumerate()
    {
        let tree = i == 7;
        let source = if tree {
            "extracted/native-props/tree-neutral.sa2mdl".into()
        } else {
            format!(
                "extracted/cutscene-e0207/e0207/Scene 2/Entity {} Model.sa2mdl",
                i + 2
            )
        };
        let visual =
            evaluate_static_model(&read_model(&fs::read(local.join(source)).unwrap()).unwrap())
                .unwrap();
        let mappings: Vec<_> = if tree {
            (4..=6)
                .map(|index| {
                    (
                        index,
                        format!("al_neut_obj_tex/gno_ki_def0{}.png", index - 3),
                    )
                })
                .collect()
        } else {
            vec![((i + 2) as u16, format!("e0207texture/chaoseme_{name}.png"))]
        };
        let pngs: BTreeMap<_, _> = mappings
            .into_iter()
            .map(|(id, file)| {
                (
                    id,
                    fs::read(local.join("extracted/textures").join(file)).unwrap(),
                )
            })
            .collect();
        let file = export(&visual, &pngs).unwrap();
        let u32_at = |i| u32::from_le_bytes(file[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!(&file[..4], b"glTF");
        assert_eq!(u32_at(4), 2);
        assert_eq!(u32_at(8), file.len());
        assert_eq!(&file[16..20], b"JSON");
        let n = u32_at(12);
        let doc: Value = serde_json::from_slice(&file[20..20 + n]).unwrap();
        assert_eq!(&file[24 + n..28 + n], b"BIN\0");
        let bin = &file[28 + n..];
        assert_eq!(bin.len(), u32_at(20 + n));
        for img in doc["images"].as_array().unwrap() {
            let bytes = section(&doc, img["bufferView"].as_u64().unwrap() as usize, bin);
            let id = img["extras"]["sa2TextureIndex"].as_u64().unwrap() as u16;
            assert_eq!(bytes, pngs[&id]);
            assert_eq!(img["extras"]["sha256"], sha256(&bytes));
        }
        let primitives = doc["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(primitives.len(), visual.surfaces.len());
        for (prim, surface) in primitives.iter().zip(&visual.surfaces) {
            let attr = &prim["attributes"];
            assert_eq!(attr["TEXCOORD_0"].is_number(), tree);
            assert_eq!(attr["COLOR_0"].is_number(), tree);
            assert_eq!(attr["NORMAL"].is_number(), !tree);
            let m = &doc["materials"][prim["material"].as_u64().unwrap() as usize];
            assert_eq!(m["extras"]["sa2Chunk"]["requiresEnvironmentShader"], !tree);
            let pbr = &m["pbrMetallicRoughness"];
            assert_eq!(pbr["baseColorTexture"].is_object(), tree);
            assert!(
                (pbr["baseColorFactor"][3].as_f64().unwrap()
                    - f64::from(surface.material.diffuse_rgba.unwrap()[3]) / 255.)
                    .abs()
                    < 1e-12
            );
            let check = |name: &str, expected: Vec<f32>, dimension: usize| {
                if expected.is_empty() {
                    assert!(attr[name].is_null());
                    return;
                }
                let accessor = &doc["accessors"][attr[name].as_u64().unwrap() as usize];
                assert_eq!(accessor["componentType"], 5126);
                assert_eq!(
                    accessor["count"].as_u64().unwrap() as usize * dimension,
                    expected.len()
                );
                let bytes = section(&doc, accessor["bufferView"].as_u64().unwrap() as usize, bin);
                let actual: Vec<_> = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .map(f32::from_le_bytes)
                    .collect();
                assert_eq!(actual.len(), expected.len());
                assert!(
                    actual
                        .iter()
                        .zip(expected)
                        .all(|(a, b)| (*a - b).abs() < 1e-6),
                    "{name} differs"
                );
            };
            let mut positions = Vec::new();
            let mut normals = Vec::new();
            let mut uvs = Vec::new();
            let mut colors = Vec::new();
            for strip in &surface.strips {
                for face in &strip.triangles {
                    for i in face {
                        let corner = &strip.corners[*i];
                        let vertex = &visual.vertices[corner.vertex];
                        positions.extend(vertex.world_position.map(|x| x as f32));
                        if let Some(n) = vertex.world_normal {
                            normals.extend(n.map(|x| x as f32));
                        }
                        if let Some(uv) = corner.uv {
                            uvs.extend(uv);
                        }
                        if let Some(rgba) = vertex.diffuse_rgba {
                            for (i, byte) in rgba.into_iter().enumerate() {
                                let x = f64::from(byte) / 255.;
                                let linear = if i == 3 {
                                    x
                                } else if x <= 0.04045 {
                                    x / 12.92
                                } else {
                                    ((x + 0.055) / 1.055).powf(2.4)
                                };
                                colors.push(linear as f32);
                            }
                        }
                    }
                }
            }
            check("POSITION", positions, 3);
            check("NORMAL", normals, 3);
            check("TEXCOORD_0", uvs, 2);
            check("COLOR_0", colors, 4);
        }
        assert!(export(&visual, &BTreeMap::new()).is_err());
        let mut corrupt = pngs.clone();
        *corrupt.values_mut().next().unwrap() = vec![0; 12];
        assert!(export(&visual, &corrupt).is_err());
    }
}
#[test]
#[ignore = "requires the original blue emerald and extracted texture"]
fn binding_file_verifies_model_indices_hashes_and_relative_paths() {
    use sa2_asset_tools::glb_cli::build;
    use serde_json::json;
    let local = PathBuf::from(std::env::var_os("SA2_VISUAL_LOCAL").unwrap());
    let model = read_model(
        &fs::read(local.join("extracted/cutscene-e0207/e0207/Scene 2/Entity 2 Model.sa2mdl"))
            .unwrap(),
    )
    .unwrap();
    let png = fs::read(local.join("extracted/textures/e0207texture/chaoseme_blue.png")).unwrap();
    let dir = local.join(format!(
        "sa2-visual-binding-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("bindings.json");
    let file = dir.join("texture.png");
    fs::write(&file, &png).unwrap();
    let entry = json!({"index":2,"path":"texture.png","sha256":sha256(&png)});
    let set = |entries: Vec<Value>| {
        fs::write(
            &path,
            serde_json::to_vec(&json!({"textures":entries})).unwrap(),
        )
        .unwrap()
    };
    set(vec![entry.clone()]);
    let (glb, deps) = build(&model, &path).unwrap();
    assert_eq!(&glb[..4], b"glTF");
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[1].bytes, png);
    assert_eq!(deps[1].role, "native-texture-index-2");
    assert_eq!(deps[1].logical_path, "bindings/texture.png");
    set(vec![entry.clone(), entry.clone()]);
    assert!(build(&model, &path).is_err());
    for (key, value) in [
        ("path", json!("../texture.png")),
        ("index", json!(3)),
        ("sha256", json!("0".repeat(64))),
    ] {
        let mut bad = entry.clone();
        bad[key] = value;
        set(vec![bad]);
        assert!(build(&model, &path).is_err());
    }
    fs::remove_file(file).unwrap();
    fs::remove_file(path).unwrap();
    fs::remove_dir(dir).unwrap();
}
