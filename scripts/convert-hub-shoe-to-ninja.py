"""Convert the hub's Shadow Air Shoes (shadow-air-shoes-*.glb, made by
scripts/convert-upgrade-assets.py from TERIOSMDL 65 heel/toe + overlay 78/79
and TERIOSTEX) into a Ninja chunk vertex list and polygon list the game draws
on the player's Chao (sa2-boot chao_player.rs hub_shoe).

Only the shoe and overlay meshes (glTF meshes 0 and 1) are taken; the jet
meshes stay with the game's own jet models. Positions, normals, UVs and the
native texture index (material extras sa2Source.textureIndex) are copied as
they are. The output is local game-derived data: never commit it.

usage: python3 scripts/convert-hub-shoe-to-ninja.py IN.glb OUT.njsh

OUT layout (big-endian): b"NJSH", u32 vertex count, u32 vlist bytes,
u32 plist bytes, then the vlist chunks, then the plist (u16 stream).
"""
import json
import struct
import sys

CV_VN = 41  # NJD_CV_VN: position + normal
CT_TID = 8  # NJD_CT_TID: texture id
CS_UVN = 65  # NJD_CS_UVN: strip, UV in 0..255
FST_DB = 0x10  # strip flag: double sided (glTF winding not checked against Ninja's)
PER_CHUNK = 64


def read_glb(path):
    b = open(path, 'rb').read()
    n = struct.unpack('<I', b[12:16])[0]
    j = json.loads(b[20:20 + n])
    off = 20 + n
    bin_len = struct.unpack('<I', b[off:off + 4])[0]
    blob = b[off + 8:off + 8 + bin_len]
    return j, blob


def accessor(j, blob, i):
    a = j['accessors'][i]
    v = j['bufferViews'][a['bufferView']]
    start = v.get('byteOffset', 0) + a.get('byteOffset', 0)
    comps = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4}[a['type']]
    fmt = {5126: 'f', 5123: 'H', 5125: 'I'}[a['componentType']]
    size = struct.calcsize('<' + fmt)
    stride = v.get('byteStride', size * comps)
    return [struct.unpack_from('<' + fmt * comps, blob, start + k * stride) for k in range(a['count'])]


def convert(path):
    j, blob = read_glb(path)
    verts, prims = [], []
    for mesh in j['meshes'][:2]:
        for p in mesh['primitives']:
            pos = accessor(j, blob, p['attributes']['POSITION'])
            nor = accessor(j, blob, p['attributes']['NORMAL'])
            uv = accessor(j, blob, p['attributes']['TEXCOORD_0'])
            idx = [t[0] for t in accessor(j, blob, p['indices'])]
            tex = j['materials'][p['material']]['extras']['sa2Source']['textureIndex']
            base = len(verts)
            # The hub's glTF root mirrors the source X (src/chao-attachments.js):
            # mirrored back, its mount turns the shoe flat (height 2.6, length 5.1).
            verts += [((-pos[k][0], pos[k][1], pos[k][2]), (-nor[k][0], nor[k][1], nor[k][2])) for k in range(len(pos))]
            prims.append((tex, [(base + idx[k], uv[idx[k]]) for k in range(len(idx))]))
    vl = b''
    for start in range(0, len(verts), PER_CHUNK):
        part = verts[start:start + PER_CHUNK]
        vl += struct.pack('>II', ((1 + 6 * len(part)) << 16) | (0x80 << 8) | CV_VN, (len(part) << 16) | start)
        for p, n in part:
            vl += struct.pack('>6f', *p, *n)
    vl += struct.pack('>I', 0xFF)
    pl = []
    for tex, corners in prims:
        pl += [CT_TID, tex & 0x1FFF]
        tris = [corners[k:k + 3] for k in range(0, len(corners), 3)]
        body = [len(tris)]
        for tri in tris:
            body.append(3)
            for i, (u, v) in tri:
                body += [i, max(0, min(65535, round(u * 255))) & 0xFFFF, max(0, min(65535, round(v * 255))) & 0xFFFF]
        pl += [(FST_DB << 8) | CS_UVN, len(body)] + body
    pl.append(0xFF)
    plb = struct.pack('>%dH' % len(pl), *pl)
    return b'NJSH' + struct.pack('>III', len(verts), len(vl), len(plb)) + vl + plb


def self_check():
    # One triangle in, one strip chunk out: the header counts add up.
    import os
    import tempfile
    pos = struct.pack('<9f', 0, 0, 0, 1, 0, 0, 0, 1, 0)
    nor = struct.pack('<9f', *([0, 0, 1] * 3))
    uv = struct.pack('<6f', 0, 0, 1, 0, 0, 1)
    ind = struct.pack('<3H', 0, 1, 2) + b'\0\0'
    blob = pos + nor + uv + ind
    views = [{'buffer': 0, 'byteOffset': o, 'byteLength': l} for o, l in ((0, 36), (36, 36), (72, 24), (96, 6))]
    acc = [{'bufferView': 0, 'componentType': 5126, 'count': 3, 'type': 'VEC3'}, {'bufferView': 1, 'componentType': 5126, 'count': 3, 'type': 'VEC3'},
           {'bufferView': 2, 'componentType': 5126, 'count': 3, 'type': 'VEC2'}, {'bufferView': 3, 'componentType': 5123, 'count': 3, 'type': 'SCALAR'}]
    j = {'accessors': acc, 'bufferViews': views, 'meshes': [{'primitives': [{'attributes': {'POSITION': 0, 'NORMAL': 1, 'TEXCOORD_0': 2}, 'indices': 3, 'material': 0}]}],
         'materials': [{'extras': {'sa2Source': {'textureIndex': 6}}}]}
    js = json.dumps(j).encode()
    js += b' ' * (-len(js) % 4)
    glb = b'glTF' + struct.pack('<II', 2, 0) + struct.pack('<II', len(js), 0x4E4F534A) + js + struct.pack('<II', len(blob), 0x004E4942) + blob
    fd, tmp = tempfile.mkstemp(suffix='.glb')
    os.write(fd, glb)
    os.close(fd)
    out = convert(tmp)
    os.remove(tmp)
    n, vlen, plen = struct.unpack('>III', out[4:16])
    assert out[:4] == b'NJSH' and n == 3 and vlen == 8 + 3 * 24 + 4
    pl = struct.unpack('>%dH' % (plen // 2), out[16 + vlen:])
    assert pl == (CT_TID, 6, (FST_DB << 8) | CS_UVN, 11, 1, 3, 0, 0, 0, 1, 255, 0, 2, 0, 255, 0xFF), pl


if __name__ == '__main__':
    self_check()
    if len(sys.argv) == 3:
        data = convert(sys.argv[1])
        open(sys.argv[2], 'wb').write(data)
        print(f'{sys.argv[2]}: {len(data)} bytes')
