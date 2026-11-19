use sa2_asset_tools::{
    model::read_model,
    visual::{evaluate_static_model, StaticVisual},
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Only the accessor types emitted by the pinned local Blender conversion.
struct Glb {
    doc: Value,
    bytes: Vec<u8>,
}
impl Glb {
    fn load(path: &Path) -> Self {
        let file = fs::read(path).unwrap();
        assert_eq!(&file[..4], b"glTF");
        let n = u32::from_le_bytes(file[12..16].try_into().unwrap()) as usize;
        let doc = serde_json::from_slice(&file[20..20 + n]).unwrap();
        assert_eq!(&file[24 + n..28 + n], b"BIN\0");
        Self {
            doc,
            bytes: file[28 + n..].to_vec(),
        }
    }
    fn read(&self, accessor: &Value, index: usize) -> Vec<f64> {
        let a = &self.doc["accessors"][accessor.as_u64().unwrap() as usize];
        assert!(index < a["count"].as_u64().unwrap() as usize);
        let view = &self.doc["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let n = match a["type"].as_str().unwrap() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => panic!("reference accessor"),
        };
        let component = a["componentType"].as_u64().unwrap();
        let width = match component {
            5126 => 4,
            5123 => 2,
            _ => panic!("reference component"),
        };
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize
            + index * view["byteStride"].as_u64().unwrap_or((n * width) as u64) as usize;
        (0..n)
            .map(|k| {
                let p = offset + k * width;
                if component == 5126 {
                    f32::from_le_bytes(self.bytes[p..p + 4].try_into().unwrap()) as f64
                } else {
                    u16::from_le_bytes(self.bytes[p..p + 2].try_into().unwrap()) as f64
                }
            })
            .collect()
    }
}

type Triangle = [[f64; 12]; 3];
fn transform(node: &Value, v: &[f64]) -> [f64; 3] {
    let component = |key: &str, i: usize, default: f64| {
        node[key].get(i).and_then(Value::as_f64).unwrap_or(default)
    };
    assert!(node["matrix"].is_null() && node["scale"].is_null());
    let q: [f64; 3] = std::array::from_fn(|i| component("rotation", i, 0.));
    let w = component("rotation", 3, 1.);
    let cross = |a: &[f64], b: &[f64]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let t = cross(&q, v).map(|x| 2. * x);
    let u = cross(&q, &t);
    std::array::from_fn(|i| v[i] + w * t[i] + u[i] + component("translation", i, 0.))
}

fn compare(glb: &Glb, visual: &StaticVisual) -> usize {
    let root = glb.doc["scenes"][0]["nodes"][0].as_u64().unwrap() as usize;
    let mut nodes = vec![root];
    if let Some(children) = glb.doc["nodes"][root]["children"].as_array() {
        nodes.extend(children.iter().map(|v| v.as_u64().unwrap() as usize));
    }
    let mut surfaces = visual.surfaces.iter();
    let mut total = 0;
    for index in nodes {
        let node = &glb.doc["nodes"][index];
        let Some(mesh) = node["mesh"].as_u64() else {
            continue;
        };
        for prim in glb.doc["meshes"][mesh as usize]["primitives"]
            .as_array()
            .unwrap()
        {
            let surface = surfaces.next().unwrap();
            let mat = &glb.doc["materials"][prim["material"].as_u64().unwrap() as usize]["extras"]
                ["sa2Source"];
            assert_eq!(
                mat["textureIndex"].as_u64(),
                surface.material.texture_id.map(u64::from)
            );
            let diffuse = surface.material.diffuse_rgba.unwrap();
            for (i, byte) in diffuse.into_iter().enumerate() {
                assert!(
                    (f64::from(byte) / 255. - mat["diffuse"][i].as_f64().unwrap()).abs() < 1e-6
                );
            }
            for (bit, field) in [(8, "useAlpha"), (16, "doubleSided"), (64, "environment")] {
                assert_eq!(mat[field].as_bool(), Some(surface.strip_flags & bit != 0));
            }
            assert_eq!(surface.material.blend_flags, Some(0x25));
            assert_eq!(mat["sourceAlpha"], "SRC");
            assert_eq!(mat["destinationAlpha"], "INV_SRC");
            let mut actual: Vec<Triangle> = Vec::new();
            for strip in &surface.strips {
                for face in &strip.triangles {
                    actual.push(face.map(|i| {
                        let c = &strip.corners[i];
                        let vertex = &visual.vertices[c.vertex];
                        let [x, y, z] = vertex.world_position;
                        let mut normal = vertex.local_normal.unwrap_or_default().map(f64::from);
                        let len = normal.iter().map(|x| x * x).sum::<f64>().sqrt();
                        if len > 0. {
                            normal = normal.map(|x| x / len);
                        }
                        let color = vertex
                            .diffuse_rgba
                            .unwrap_or_default()
                            .map(|v| f64::from(v) / 255.);
                        let linear = |srgb: f64| {
                            if srgb <= 0.04045 {
                                srgb / 12.92
                            } else {
                                ((srgb + 0.055) / 1.055).powf(2.4)
                            }
                        };

                        let [u, v] = c.uv.unwrap_or_default();
                        [
                            x,
                            y,
                            z,
                            f64::from(u),
                            f64::from(v),
                            linear(color[0]),
                            linear(color[1]),
                            linear(color[2]),
                            color[3],
                            normal[0],
                            normal[1],
                            normal[2],
                        ]
                    }));
                }
            }
            let indices = &prim["indices"];
            let count = glb.doc["accessors"][indices.as_u64().unwrap() as usize]["count"]
                .as_u64()
                .unwrap() as usize;
            assert_eq!(actual.len() * 3, count);
            for i in (0..count).step_by(3) {
                let reference: Triangle = std::array::from_fn(|j| {
                    let idx = glb.read(indices, i + j)[0] as usize;
                    let attr = &prim["attributes"];
                    let p = glb.read(&attr["POSITION"], idx);
                    let mut p = transform(node, &p);
                    if index != root {
                        p = transform(&glb.doc["nodes"][root], &p);
                    }
                    let uv = glb.read(&attr["TEXCOORD_0"], idx);
                    let (color, normal) = if attr["COLOR_0"].is_null() {
                        (vec![0.; 4], glb.read(&attr["NORMAL"], idx))
                    } else {
                        (
                            glb.read(&attr["COLOR_0"], idx)
                                .into_iter()
                                .map(|x| x / 65535.)
                                .collect(),
                            vec![0.; 3],
                        )
                    };
                    [
                        p[0], p[1], p[2], uv[0], uv[1], color[0], color[1], color[2], color[3],
                        normal[0], normal[1], normal[2],
                    ]
                });
                let matches = |a: &Triangle| {
                    (0..3).any(|shift| {
                        (0..3).all(|j| {
                            (0..12).all(|k| {
                                (a[(j + shift) % 3][k] - reference[j][k]).abs()
                                    < if k >= 9 { 0.0005 } else { 0.00005 }
                            })
                        })
                    })
                };
                let found=actual.iter().position(matches).unwrap_or_else(||panic!("wrong wound face/UV at node {index} index {i}: expected {reference:?}, first {:?}",actual.first()));
                actual.swap_remove(found);
                total += 1;
            }
        }
    }
    assert!(surfaces.next().is_none());
    total
}

#[test]
#[ignore = "requires locally extracted original models and their independently exported Blender GLBs"]
fn static_surfaces_match_blender_exports() {
    let local = PathBuf::from(
        std::env::var_os("SA2_VISUAL_LOCAL")
            .expect("set SA2_VISUAL_LOCAL to the original checkout's .local directory"),
    );
    let reference_hashes = [
        (
            "e7189e4f08f786fdcd306a1386c5acfd25580ecc77433ff6b9b4ec3f715944e3",
            "24f521669881c44aa0c6d729f2748a6883e8e5f677d06cdbf5fd87828f9868ea",
        ),
        (
            "967d429eae9c71b88cd13ad415f3f10d44c291b33f6ac2dd0d2849fe380c6b9d",
            "d29c74548f7c60f1dfa4a644c665a4560750ded499d9daf9d2a78ee1a8362542",
        ),
        (
            "f30e07fc5ad431f48d4d7874536c07b1f1917d42aa813bf562758b664cc41240",
            "85bf07dfbca806bd74cc3cd6827052234119e14da8e1785e4a927d6d4df20f37",
        ),
        (
            "a480351b62a99b80c753300af9f575642a2e6bd3c0bb1dea3377c649a836473e",
            "d121a00b9b875b1425f469fe55e9c5087b5db3f53b06dadf87764c1faba5575c",
        ),
        (
            "92af2e3b666477cb88a35e43c5cd576b44735603cc74978a1eff638981e35ae8",
            "d6c05784c2c7d53bb22e4109a35ded4b1de9881627157205e4b5c28ed3aa17f5",
        ),
        (
            "ad844f9e8203c1b594a8a5176aae76eb60bb2bc3c8e88f742d067d5fa55086a8",
            "125ee2041bb69fd508c4ac8591d6378d8de0af0e233e1ec46ab7161eb5ef6bf8",
        ),
        (
            "b68108838db0e274b3fee664493adc241607940b05b15aa4e7babd38de144775",
            "7d22e93164931afbbffee0701c341b801ca5d61b213ef7721e41b54187ab74cb",
        ),
        (
            "22cf9d904e566fb4e068347565baa591656db4c079dbb8e104e19a2feac5e4e5",
            "e095f2cc7eb92d45e576d60ffd36868944221410e070a2f67dd540f4b6cb447b",
        ),
    ];
    let mut total = 0;
    for (i, name) in [
        "emerald-blue",
        "emerald-green",
        "emerald-cyan",
        "emerald-purple",
        "emerald-red",
        "emerald-white",
        "emerald-yellow",
        "tree-neutral",
    ]
    .iter()
    .enumerate()
    {
        let source = if i == 7 {
            "extracted/native-props/tree-neutral.sa2mdl".into()
        } else {
            format!(
                "extracted/cutscene-e0207/e0207/Scene 2/Entity {} Model.sa2mdl",
                i + 2
            )
        };
        let glb_path = local.join(format!("imports/{name}.glb"));
        assert_eq!(
            sa2_asset_tools::manifest::file_hash(&glb_path).unwrap().1,
            reference_hashes[i].1
        );
        let glb = Glb::load(&glb_path);
        let bytes = fs::read(local.join(source)).unwrap();
        assert_eq!(
            sa2_asset_tools::manifest::sha256(&bytes),
            reference_hashes[i].0
        );
        let model = read_model(&bytes).unwrap();
        let visual = evaluate_static_model(&model).unwrap();
        let count = compare(&glb, &visual);
        eprintln!("{name}: {count} wound triangles");
        total += count;
    }
    assert_eq!(total, 670);
    eprintln!("total {total} wound triangles");
}
