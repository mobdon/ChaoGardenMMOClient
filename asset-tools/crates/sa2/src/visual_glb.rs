//! Standalone glTF 2.0 binary export for independently checked static Chunk profiles.
use crate::manifest::sha256;
use crate::visual::{StaticVisual, Surface};
use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_BIN: usize = 128 * 1024 * 1024;
#[derive(Default)]
struct Buffers {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}
impl Buffers {
    fn view(&mut self, data: &[u8], target: Option<u32>) -> Result<usize> {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        if data.len() > MAX_BIN - self.bytes.len() {
            return Err(Error::invalid("GLB buffer limit"));
        }
        let mut view = json!({"buffer":0,"byteOffset":self.bytes.len(),"byteLength":data.len()});
        if let Some(target) = target {
            view["target"] = target.into();
        }
        let index = self.views.len();
        self.views.push(view);
        self.bytes.extend_from_slice(data);
        Ok(index)
    }
    fn attribute<const N: usize>(
        &mut self,
        values: &[[f32; N]],
        kind: &str,
        bounds: bool,
    ) -> Result<usize> {
        let size = values
            .len()
            .checked_mul(N * 4)
            .ok_or_else(|| Error::invalid("attribute overflow"))?;
        if size > MAX_BIN - self.bytes.len() {
            return Err(Error::invalid("GLB attribute limit"));
        }
        let mut data = Vec::with_capacity(size);
        let mut min = [f32::INFINITY; N];
        let mut max = [f32::NEG_INFINITY; N];
        for value in values {
            for (i, v) in value.iter().enumerate() {
                if !v.is_finite() {
                    return Err(Error::invalid("non-finite GLB attribute"));
                }
                data.extend_from_slice(&v.to_le_bytes());
                min[i] = min[i].min(*v);
                max[i] = max[i].max(*v);
            }
        }
        let view = self.view(&data, Some(34962))?;
        let mut acc =
            json!({"bufferView":view,"componentType":5126,"count":values.len(),"type":kind});
        if bounds {
            acc["min"] = json!(min.as_slice());
            acc["max"] = json!(max.as_slice());
        }
        let index = self.accessors.len();
        self.accessors.push(acc);
        Ok(index)
    }
}
fn linear(byte: u8) -> f32 {
    let v = f32::from(byte) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn wrap(flags: u8, clamp: u8, mirror: u8) -> u32 {
    if flags & clamp != 0 {
        33071
    } else if flags & mirror != 0 {
        33648
    } else {
        10497
    }
}
fn validate_png(bytes: &[u8]) -> Result<()> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::invalid(format!("invalid source PNG: {e}")))?;
    let info = reader.info();
    if info.animation_control.is_some()
        || info.width > 8192
        || info.height > 8192
        || reader.output_buffer_size() > 32 * 1024 * 1024
    {
        return Err(Error::invalid(
            "unsupported source PNG dimensions or animation",
        ));
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut pixels)
        .map_err(|e| Error::invalid(format!("invalid source PNG pixels: {e}")))?;
    reader
        .finish()
        .map_err(|e| Error::invalid(format!("invalid source PNG end: {e}")))?;
    Ok(())
}

fn geometry(buffers: &mut Buffers, visual: &StaticVisual, surface: &Surface) -> Result<Value> {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    for strip in &surface.strips {
        for face in &strip.triangles {
            for i in face {
                let corner = strip
                    .corners
                    .get(*i)
                    .ok_or_else(|| Error::invalid("invalid GLB corner"))?;
                let v = visual
                    .vertices
                    .get(corner.vertex)
                    .ok_or_else(|| Error::invalid("invalid GLB vertex"))?;
                if positions.len() >= 1_000_000 {
                    return Err(Error::invalid("GLB surface vertex limit"));
                }
                positions.push(v.world_position.map(|x| x as f32));
                if let Some(n) = v.world_normal {
                    normals.push(n.map(|x| x as f32));
                }
                if let Some(uv) = corner.uv {
                    uvs.push(uv);
                }
                if let Some([r, g, b, a]) = v.diffuse_rgba {
                    colors.push([linear(r), linear(g), linear(b), f32::from(a) / 255.]);
                }
            }
        }
    }
    let count = positions.len();
    if count == 0
        || [normals.len(), uvs.len(), colors.len()]
            .iter()
            .any(|n| *n != 0 && *n != count)
    {
        return Err(Error::invalid(
            "empty or mixed GLB vertex attribute profile",
        ));
    }
    let env = surface.strip_flags & 64 != 0;
    if env && normals.is_empty() || !env && surface.strip_kind != 64 && uvs.is_empty() {
        return Err(Error::invalid("missing native projection attributes"));
    }
    let mut attributes = json!({"POSITION":buffers.attribute(&positions,"VEC3",true)?});
    if !normals.is_empty() {
        attributes["NORMAL"] = buffers.attribute(&normals, "VEC3", false)?.into();
    }
    if !uvs.is_empty() {
        attributes["TEXCOORD_0"] = buffers.attribute(&uvs, "VEC2", false)?.into();
    }
    if !colors.is_empty() {
        attributes["COLOR_0"] = buffers.attribute(&colors, "VEC4", false)?.into();
    }
    Ok(attributes)
}

/// The caller resolves PNGs by original texture-list index and validates the
/// expected digest from its input manifest. Each embedded image records its actual digest.
pub fn export(visual: &StaticVisual, pngs: &BTreeMap<u16, Vec<u8>>) -> Result<Vec<u8>> {
    if visual.surfaces.len() > crate::visual::MAX_SURFACES {
        return Err(Error::invalid("GLB surface metadata limit"));
    }
    let used: BTreeSet<_> = visual
        .surfaces
        .iter()
        .map(|s| {
            s.material
                .texture_id
                .ok_or_else(|| Error::invalid("missing native texture index"))
        })
        .collect::<Result<_>>()?;
    if used.is_empty() || !used.iter().copied().eq(pngs.keys().copied()) {
        return Err(Error::invalid(
            "GLB requires exactly the referenced native texture mappings",
        ));
    }
    let mut buffers = Buffers::default();
    let mut images = Vec::new();
    let mut image_for_id = BTreeMap::new();
    for (&id, bytes) in pngs {
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(Error::invalid("PNG byte limit"));
        }
        validate_png(bytes)?;
        image_for_id.insert(id, images.len());
        images.push(json!({"mimeType":"image/png","bufferView":buffers.view(bytes,None)?,"extras":{"sa2TextureIndex":id,"sha256":sha256(bytes)}}));
    }
    let mut materials = Vec::new();
    let mut primitives = Vec::new();
    let mut textures = Vec::new();
    let mut samplers = Vec::new();
    for surface in &visual.surfaces {
        let mat = &surface.material;
        if !matches!(
            surface.strip_flags,
            0x02 | 0x03 | 0x0a | 0x0b | 0x12 | 0x1a | 0x58
        ) || mat.blend_flags != Some(0x25)
        {
            return Err(Error::invalid("unsupported GLB material/strip profile"));
        }
        let [h, data] = mat
            .texture_words
            .ok_or_else(|| Error::invalid("missing GLB texture state"))?;
        let flags = (h >> 8) as u8;
        if data >> 14 != 1 || data & 0x2000 != 0 {
            return Err(Error::invalid("unsupported texture filtering"));
        }
        let id = mat
            .texture_id
            .ok_or_else(|| Error::invalid("missing texture"))?;
        let sampler = samplers.len();
        samplers.push(json!({"wrapS":wrap(flags,32,128),"wrapT":wrap(flags,16,64),"magFilter":9729,"minFilter":9729}));
        let texture = textures.len();
        textures.push(json!({"source":image_for_id[&id],"sampler":sampler}));
        let diffuse = mat
            .diffuse_rgba
            .ok_or_else(|| Error::invalid("missing diffuse material"))?
            .map(|x| f64::from(x) / 255.);
        let env = surface.strip_flags & 64 != 0;
        let mut pbr = json!({"baseColorFactor":diffuse,"metallicFactor":0.0});
        if !env && surface.strip_kind != 64 {
            pbr["baseColorTexture"] = json!({"index":texture});
        }
        let material = materials.len();
        materials.push(json!({
            "pbrMetallicRoughness":pbr,"alphaMode":if surface.strip_flags&8!=0 {"BLEND"}else{"OPAQUE"},
            "doubleSided":surface.strip_flags&16!=0,
            "extras":{"sa2Chunk":{"material":mat,"stripFlags":surface.strip_flags,"texture":texture,
                "textureCoordinates":if env {"cameraSpaceNormal.xy * 0.5 + 0.5"}else if surface.strip_kind==64 {"no authored UV"} else{"TEXCOORD_0"},
                "requiresEnvironmentShader":env}}
        }));
        if surface.strip_flags & 1 != 0 {
            materials[material]["extensions"] = json!({"KHR_materials_unlit":{}});
        }
        primitives.push(json!({"attributes":geometry(&mut buffers,visual,surface)?,"material":material,"mode":4,
            "extras":{"sa2NodeOffset":surface.node_offset,"sa2AttachmentOffset":surface.attachment_offset,"sa2PolygonOffset":surface.polygon_offset}}));
    }
    let unlit = visual.surfaces.iter().any(|s| s.strip_flags & 1 != 0);
    let mut doc = json!({"asset":{"version":"2.0","generator":"sa2-asset-tools"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
        "meshes":[{"primitives":primitives}],"materials":materials,"textures":textures,"samplers":samplers,"images":images,
        "buffers":[{"byteLength":buffers.bytes.len()}],"bufferViews":buffers.views,"accessors":buffers.accessors});
    if unlit {
        doc["extensionsUsed"] = json!(["KHR_materials_unlit"]);
    }
    let mut json = serde_json::to_vec(&doc)?;
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while buffers.bytes.len() % 4 != 0 {
        buffers.bytes.push(0);
    }
    let size = u32::try_from(28 + json.len() + buffers.bytes.len())
        .map_err(|_| Error::invalid("GLB size overflow"))?;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"glTF");
    for word in [2, size, json.len() as u32] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(b"JSON");
    out.extend(json);
    out.extend_from_slice(&(buffers.bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend(buffers.bytes);
    Ok(out)
}
