"""Metal Sonic Chao parts, from the player's own Steam files (never in Git).

python3 scripts/metal-chao/convert.py "<Sonic Adventure 2 install>"
Reads resource/gd_PC/metalsonicmdl.prs (model 418, Metal Sonic), metalsonictex.prs and
DLL/Win32/Data_DLL.dll (AL_RootObject, the Chao). Writes .local/imports/metal-chao/:
one GLB per part in its Chao node's own frame (parented like a worn hat), the body skin
texture, and parts.json. Part sizes and spots are styling choices (2026-11-21), not
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
STYLE = {'height': 1.0, 'arms': 0.8, 'armSpread': 45, 'legs': 0.75, 'belly': [1, 1, 1], 'head': 1.45, 'headY': 0.0, 'headZ': 0.0, 'skullBack': 0.5, 'coreY': 0.45, 'jetY': 0.5}
SKIN_TEXTURE = 3  # metalsonictex entry 3: Metal Sonic's blue environment-mapped metal
HIDE_NODES = list(range(40))  # the whole Chao is drawn as Metal Sonic; the Chao rig still moves him

def tool(*args):
    run = subprocess.run([TOOL, *map(str, args)], capture_output=True, text=True)
    if run.returncode: raise SystemExit(f'sa2-assets {args[0]} failed: {run.stderr.strip()}')

def prepare():
    WORK.mkdir(parents=True, exist_ok=True); OUT.mkdir(parents=True, exist_ok=True)
    for bank in ('metalsonicmdl', 'metalsonicmtn', 'plcommtn'):  # his model, his motions, the shared player motions
        if not (WORK / f'{bank}.bin').exists(): tool('prs', GD / f'{bank}.prs', WORK / f'{bank}.bin')
    if not (WORK / 'tex').exists(): tool('extract', GD / 'metalsonictex.prs', WORK / 'tex')
    for i in range(13):
        png = WORK / f'metal-{i:05d}.png'
        if not png.exists(): tool('texture', WORK / f'tex/{i:05d}.gvr', png)

def metal(pose=None, anim=None):
    d = (WORK / 'metalsonicmdl.bin').read_bytes(); i = 0
    while True:
        k, o = struct.unpack('>iI', d[i:i + 8])
        if k == -1: raise SystemExit('Metal Sonic model 418 not in metalsonicmdl.prs')
        if k == 418: return Model(lambda a, n: d[a:a + n], '>', o, pose, anim)
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
    # The Chao body mesh shrunk about its own node (smaller belly); the client scales that mesh the same.
    o = W[:3, 3]; k = np.array(STYLE['belly'])  # slimmer more than shorter, so the limbs stay attached
    return {**surface, 'tris': [[(o + (v[0] - o) * k, v[1], v[2]) for v in t] for t in surface['tris']]}

def build():
    C = chao(); CS = C.evaluate(); CW = {n['i']: n['W'] for n in C.nodes}
    CS = [belly(s, CW[1]) if s['node'] == 1 else s for s in CS]
    def box(nodes):
        P = points([s for s in CS if s['node'] in nodes]); return P.min(0), P.max(0)
    # Metal Sonic's own whole body in Chao form, proportions mixed.
    # Posed and proportioned through his own skeleton, so his weighted skin bends and stretches at the joints
    # instead of tearing: head scaled at the neck, arms at the shoulders, legs at the hips.
    pose = {44: np.diag([STYLE['head']] * 3 + [1]), 37: np.diag([STYLE['arms']] * 3 + [1]), 56: np.diag([STYLE['arms']] * 3 + [1]),
            7: np.diag([STYLE['legs']] * 3 + [1]), 17: np.diag([STYLE['legs']] * 3 + [1])}
    def turn(u, v):
        u, v = u / np.linalg.norm(u), v / np.linalg.norm(v); w = np.cross(u, v); c = u @ v
        K = np.array([[0, -w[2], w[1]], [w[2], 0, -w[0]], [-w[1], w[0], 0]]); return np.eye(3) + K + K @ K / (1 + c)
    def aim(node, child, target):  # rotate a joint so the bone to its child points along target (world space)
        Wn = {n['i']: n['W'] for n in metal(pose).nodes}; W3 = Wn[node][:3, :3]
        P = np.eye(4); P[:3, :3] = np.linalg.inv(W3) @ turn(Wn[child][:3, 3] - Wn[node][:3, 3], target) @ W3
        pose[node] = pose.get(node, np.eye(4)) @ P
    # Arms held out like a normal Chao's (full arms, not crossed): straighten the elbows (bind pose bends the claws
    # in to the hips), then aim each arm along the Chao's own left arm (AL_RootObject node 3 -> 4), mirrored, lifted out.
    ca = CW[4][:3, 3] - CW[3][:3, 3]; g = np.radians(STYLE['armSpread'])
    ca = np.array([ca[0] * np.cos(g) - ca[1] * np.sin(g), ca[0] * np.sin(g) + ca[1] * np.cos(g), ca[2]])
    chao_arm = np.array([-ca[2], ca[1], ca[0]])  # Chao space -> Metal space (inverse of R)
    for sh, el, wr, side in ((37, 38, 40, 1), (56, 57, 59, -1)):
        J0 = {n['i']: n['W'][:3, 3] for n in metal(pose).nodes}
        aim(el, wr, J0[el] - J0[sh]); aim(sh, el, chao_arm * [1, 1, side])
    MM = metal(pose); M = MM.evaluate(); J = {n['i']: n['W'][:3, 3] for n in MM.nodes}
    # Cut into parts that ride the Chao's nodes: each triangle goes to the region whose bone segment is nearest.
    SEG = {'torso': [(2, 34)], 'arm-right': [(56, 57), (57, 59)], 'arm-left': [(37, 38), (38, 40)],
           'leg-right': [(7, 9), (9, 13)], 'leg-left': [(17, 19), (19, 23)]}
    def dist(p, a, b):
        a, b = J[a], J[b]; t = np.clip(np.dot(p - a, b - a) / np.dot(b - a, b - a), 0, 1); return np.linalg.norm(p - (a + t * (b - a)))
    fixed = {**{i: 'head' for i in (19, 20, 21, 22, 23, 24)}, **{i: 'leg-right' for i in (1, 2, 3, 4)},
             **{i: 'leg-left' for i in (5, 6, 7, 8)}, **{i: 'arm-left' for i in (16, 17, 18)}, **{i: 'arm-right' for i in (27, 28, 29)}}
    split = {r: [] for r in [*SEG, 'head']}
    for i, surface in enumerate(M):
        if i in fixed: split[fixed[i]].append(surface); continue
        buckets = {r: [] for r in SEG}
        for t in surface['tris']:
            c = np.mean([v[0] for v in t], axis=0)
            buckets[min(SEG, key=lambda r: min(dist(c, a, b) for a, b in SEG[r]))].append(t)
        for r, tris in buckets.items():
            if tris: split[r].append({**surface, 'tris': tris})
    allp = points(M); ground = allp[:, 1].min(); height = allp[:, 1].max() - ground
    # Whole figure as tall as the Chao (crown of the Chao head to its soles), standing on the Chao's ground.
    hl, hh = box([16]); total = STYLE['height'] * hh[1] / height
    origin = np.array([J[2][0], ground, J[2][2]])
    plan = [('torso', 1), ('head', 16), ('arm-right', 10), ('arm-left', 3), ('leg-right', 13), ('leg-left', 6)]
    parts = [(region, node, split[region], total, origin, np.zeros(3), None) for region, node in plan]
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
    prepare(); build()  # build() also writes the skin texture the client hides the Chao with
    import skin; print('Wrote', skin.build(OUT / 'metal-chao-rigged.glb'))
