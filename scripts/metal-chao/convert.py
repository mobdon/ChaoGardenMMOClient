"""Metal Sonic Chao parts, from the player's own Steam files (never in Git).

python3 scripts/metal-chao/convert.py "<Sonic Adventure 2 install>"
Reads resource/gd_PC/metalsonicmdl.prs (model 418, Metal Sonic), metalsonictex.prs and
DLL/Win32/Data_DLL.dll (AL_RootObject, the Chao). Writes .local/imports/metal-chao/:
one GLB per part in its Chao node's own frame (parented like a worn hat), the body skin
texture, and parts.json. Part sizes and spots are mob's styling choices (2026-11-21), not
game data; every mesh, UV and texture pixel is the game's own.
"""
import json, os, struct, subprocess, sys
from pathlib import Path
import numpy as np, pefile
sys.path.insert(0, str(Path(__file__).parent))
from ninja import Model
import glb

ROOT = Path(__file__).resolve().parents[2]
GAME = Path(sys.argv[1] if len(sys.argv) > 1 else os.environ.get('SA2_PATH', ''))
WORK = ROOT / '.local/extracted/metal-chao'
OUT = ROOT / '.local/imports/metal-chao'
TOOL = os.environ.get('SA2_ASSETS') or str(ROOT / 'asset-tools/crates/sa2/target/release/sa2-assets')
GD = GAME / 'resource/gd_PC'
STYLE = {'belly': [0.8, 0.92, 0.8], 'shoe': 1.0, 'core': 0.45, 'head': 0.9, 'headY': 0.0, 'headZ': 0.0, 'skullBack': 0.5, 'coreY': 0.45, 'jetY': 0.5}
SKIN_TEXTURE = 3  # metalsonictex entry 3: Metal Sonic's blue environment-mapped metal
HIDE_NODES = [6, 13, 16, 17, 18, 19, 20, 21, 22, 23, 25, 27]  # Chao feet, head, eyes, ear tips and mouth: replaced by Metal Sonic's

def tool(*args):
    run = subprocess.run([TOOL, *map(str, args)], capture_output=True, text=True)
    if run.returncode: raise SystemExit(f'sa2-assets {args[0]} failed: {run.stderr.strip()}')

def prepare():
    WORK.mkdir(parents=True, exist_ok=True); OUT.mkdir(parents=True, exist_ok=True)
    tool('prs', GD / 'metalsonicmdl.prs', WORK / 'metalsonicmdl.bin') if not (WORK / 'metalsonicmdl.bin').exists() else None
    if not (WORK / 'tex').exists(): tool('extract', GD / 'metalsonictex.prs', WORK / 'tex')
    for i in range(13):
        png = WORK / f'metal-{i:05d}.png'
        if not png.exists(): tool('texture', WORK / f'tex/{i:05d}.gvr', png)

def metal():
    d = (WORK / 'metalsonicmdl.bin').read_bytes(); i = 0
    while True:
        k, o = struct.unpack('>iI', d[i:i + 8])
        if k == -1: raise SystemExit('Metal Sonic model 418 not in metalsonicmdl.prs')
        if k == 418: return Model(lambda a, n: d[a:a + n], '>', o)
        i += 8

def chao():
    pe = pefile.PE(str(GD / 'DLL/Win32/Data_DLL.dll'), fast_load=True); base = pe.OPTIONAL_HEADER.ImageBase
    get = lambda va, n: pe.get_data(va - base, n)
    export = 0x32FBD0  # AL_RootObject (see scripts/extract-native.js)
    return Model(get, '<', struct.unpack('<I', get(base + export, 4))[0])

R = np.array([[0, 0, 1], [0, 1, 0], [-1, 0, 0]], float)  # Metal faces -x, the Chao faces +z

def points(surfaces, keep=None):
    return np.array([v[0] for s in surfaces for t in s['tris'] if keep is None or keep(t) for v in t])

def placed(surfaces, scale, src, dst, keep=None, to_local=np.eye(4)):
    A = R * scale; b = dst - A @ src; L = to_local[:3, :3]; lt = to_local[:3, 3]
    Ln = np.linalg.inv(L).T
    return [{**s, 'tris': [[(L @ (A @ v[0] + b) + lt, Ln @ (R @ v[1]), v[2]) for v in t]
                           for t in s['tris'] if keep is None or keep(t)]} for s in surfaces]

def belly(surface, W):
    # The Chao body mesh shrunk about its own node (mob: belly too big); the client scales that mesh the same.
    o = W[:3, 3]; k = np.array(STYLE['belly'])  # slimmer more than shorter, so the limbs stay attached
    return {**surface, 'tris': [[(o + (v[0] - o) * k, v[1], v[2]) for v in t] for t in surface['tris']]}

def build():
    M = metal().evaluate(); C = chao(); CS = C.evaluate(); CW = {n['i']: n['W'] for n in C.nodes}
    CS = [belly(s, CW[1]) if s['node'] == 1 else s for s in CS]
    S = lambda *ix: [M[i] for i in ix]
    def box(nodes):
        P = points([s for s in CS if s['node'] in nodes]); return P.min(0), P.max(0)
    # The real Chao body stays (its own arms, hands and rig); Metal Sonic's head replaces the Chao
    # head on the head bone, his shoes replace the Chao feet, his chest core sits on the belly.
    parts = []  # (key, socket node, surfaces, scale, source anchor, destination anchor, keep)
    for side, src, node in (('right', S(1, 2, 3, 4), 13), ('left', S(5, 6, 7, 8), 6)):
        lo, hi = points(src).min(0), points(src).max(0); flo, fhi = box([node])
        sc = (fhi[2] - flo[2]) * STYLE['shoe'] / (hi[0] - lo[0])  # shoe length fits the Chao foot
        parts.append((f'shoe-{side}', node, src, sc, np.array([(lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2]),
                      np.array([(flo[0] + fhi[0]) / 2, flo[1], (flo[2] + fhi[2]) / 2]), None))
    head = S(19, 20, 21, 22, 23, 24); P = points(head); hl, hh = box([16])
    skull = P[P[:, 0] < STYLE['skullBack']]
    src = np.array([(skull[:, 0].min() + skull[:, 0].max()) / 2, P[:, 1].min(), 0])
    sc = (hh[0] - hl[0]) * STYLE['head'] / (skull[:, 2].max() - skull[:, 2].min())
    parts.append(('head', 16, head, sc, src, np.array([0, hl[1] + STYLE['headY'], (hl[2] + hh[2]) / 2 + STYLE['headZ']]), None))
    body = points([s for s in CS if s['node'] == 1]); bl, bh = body.min(0), body.max(0)
    P = points(S(13)); y = bl[1] + (bh[1] - bl[1]) * STYLE['coreY']
    band = body[(abs(body[:, 1] - y) < .3) & (abs(body[:, 0]) < .6)]
    parts.append(('core', 1, S(13), STYLE['core'], np.array([P[:, 0].max(), (P[:, 1].min() + P[:, 1].max()) / 2, (P[:, 2].min() + P[:, 2].max()) / 2]),
                  np.array([0, y, band[:, 2].max() - .03]), None))
    texfile = lambda _set, tex, _s: str(WORK / f'metal-{tex:05d}.png') if tex is not None and (WORK / f'metal-{tex:05d}.png').exists() else None
    manifest = []; local = []
    for key, node, src, sc, a, d, keep in parts:
        surfaces = placed(src, sc, a, d, keep, np.linalg.inv(CW[node])); local.append((key, node, surfaces))
        glb.write(str(OUT / f'metal-chao-{key}.glb'), [dict(name=f'metal-chao-{key}', texset='metal', surfaces=surfaces)], texfile)
        manifest.append({'key': f'metal-chao-{key}', 'node': f'{node:03d}'})
    (OUT / 'metal-chao-skin.png').write_bytes((WORK / f'metal-{SKIN_TEXTURE:05d}.png').read_bytes())
    (OUT / 'parts.json').write_text(json.dumps({'parts': manifest, 'skin': 'metal-chao-skin.png', 'hide': HIDE_NODES, 'belly': STYLE['belly'],
        'sources': {'metal': 'resource/gd_PC/metalsonicmdl.prs model 418 + metalsonictex.prs', 'chao': 'Data_DLL.dll AL_RootObject'}}, indent=1))
    print(f'Wrote {len(manifest)} Metal Sonic Chao parts to {OUT}')
    return local

if __name__ == '__main__':
    if not (GD / 'metalsonicmdl.prs').exists(): raise SystemExit('Pass the Sonic Adventure 2 install folder (Steam).')
    prepare(); build()
