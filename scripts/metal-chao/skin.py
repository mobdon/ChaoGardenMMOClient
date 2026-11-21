# Metal Sonic Chao as one rigged model: his own 62-node skeleton with the Chao-form scales (head, arms, legs),
# his skinned mesh, and his own motions as glTF clips, sized to the Chao. Local only (reads the player's Steam files).
import json, numpy as np
from pathlib import Path
import convert, glb
from motion import Bank, frame_anim
# Metal Sonic animation states (sonic2app.exe MetalSonic table, VA 0xA1A7E8, 16-byte rows) -> clip names.
STATES = {'stand': 0, 'walk': 7, 'jog': 8, 'run': 9, 'fly': 10}  # SA2 picks 7-10 by speed against jog/run/rush
# Clips are baked at one game frame per 1/60 s. The game's motion step (GSNE8P fn_8003D8D0) advances a
# mode-3 row by its speed, a mode-10 row by speed x forward speed (player +0x64) and a mode-11 row by
# speed x stored ground speed (player +0x44) (pointers set in fn_8004F838); the client applies that rate.
PHYSICS = 0x17391E8  # Steam PhysicsArray (SA2Variables.h); row 12 = Metal Sonic (GC 0x80196578 holds the same)

def rows(exe):
    import pefile, struct
    pe = pefile.PE(str(exe), fast_load=True); base = pe.OPTIONAL_HEADER.ImageBase
    out = {}
    for name, state in STATES.items():
        motion, model, mode, nxt, _ts, speed = struct.unpack('<hhhhff', pe.get_data(0xA1A7E8 + state * 16 - base, 16))
        out[name] = dict(motion=motion, mode=mode, speed=speed)
    return out

def physics(exe):
    import pefile, struct
    pe = pefile.PE(str(exe), fast_load=True)
    v = struct.unpack('<i32f', pe.get_data(PHYSICS + 12 * 0x84 - pe.OPTIONAL_HEADER.ImageBase, 0x84))
    return dict(speedMaxH=v[4], jog=v[9], run=v[10], rush=v[11], height=v[29])

def quat(m):
    m = m[:3, :3]; t = np.trace(m)
    if t > 0: s = np.sqrt(t + 1) * 2; q = [(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, s / 4]
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]: s = np.sqrt(1 + m[0, 0] - m[1, 1] - m[2, 2]) * 2; q = [s / 4, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s, (m[2, 1] - m[1, 2]) / s]
    elif m[1, 1] > m[2, 2]: s = np.sqrt(1 + m[1, 1] - m[0, 0] - m[2, 2]) * 2; q = [(m[0, 1] + m[1, 0]) / s, s / 4, (m[1, 2] + m[2, 1]) / s, (m[0, 2] - m[2, 0]) / s]
    else: s = np.sqrt(1 + m[2, 2] - m[0, 0] - m[1, 1]) * 2; q = [(m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, s / 4, (m[1, 0] - m[0, 1]) / s]
    q = np.array(q); return q / np.linalg.norm(q)

def trs(L):
    s = np.linalg.norm(L[:3, :3], axis=0); M = L.copy(); M[:3, :3] /= s
    return L[:3, 3], quat(M), s

def locals_(model):
    W = [n['W'] for n in model.nodes]
    return [np.linalg.inv(W[n['parent']]) @ W[n['i']] if n['parent'] >= 0 else W[n['i']] for n in model.nodes]

def build(out):
    S = convert.STYLE; d = lambda k: np.diag([k] * 3 + [1])
    pose = {44: d(S['head']), 37: d(S['arms']), 56: d(S['arms']), 7: d(S['legs']), 17: d(S['legs'])}
    banks = {b: Bank((convert.WORK / f'{b}.bin').read_bytes()) for b in ('metalsonicmtn', 'plcommtn')}
    motion = lambda slot: next(b for b in banks.values() if slot in b.rows).motion(slot)
    table = rows(convert.GAME / 'sonic2app.exe')
    frames, parts = motion(table['stand']['motion'])
    rest = convert.metal(pose, frame_anim(parts, 0)); M = rest.evaluate()
    P = convert.points(M); ground = P[:, 1].min(); height = P[:, 1].max() - ground
    C = convert.chao(); CS = C.evaluate(); hh = convert.points([s for s in CS if s['node'] == 16]).max(0)
    total = S['height'] * hh[1] / height; pelvis = rest.nodes[2]['W'][:3, 3]
    root = np.eye(4); root[:3, :3] = convert.R * total; root[:3, 3] = -(convert.R * total) @ np.array([pelvis[0], ground, pelvis[2]])
    # Bind = the stand pose; mesh vertices are already skinned into it, so the inverse binds are its node matrices.
    nodes = [dict(name=f'metal-{n["i"]:03d}', parent=n['parent'], **dict(zip('trs', trs(L)))) for n, L in zip(rest.nodes, locals_(rest))]
    ibm = [np.linalg.inv(n['W']) for n in rest.nodes]
    anims = []
    for name, row in table.items():
        frames, parts = motion(row['motion']); per = {i: ([], [], []) for i in range(len(nodes))}
        for f in range(frames + 1):  # last key = first frame again, so the clip loops cleanly
            for i, L in enumerate(locals_(convert.metal(pose, frame_anim(parts, f % frames)))):
                t, q, _ = trs(L); prev = per[i][2][-1] if per[i][2] else None
                if prev is not None and np.dot(prev, q) < 0: q = -q
                per[i][0].append(f / 60); per[i][1].append(t); per[i][2].append(q)
        ch = []
        for i, (times, ts, qs) in per.items():
            ch += [(i, 'translation', times, ts), (i, 'rotation', times, qs)]
        anims.append(dict(name=name, channels=ch))
    tex = lambda _s, t, _x: str(convert.WORK / f'metal-{t:05d}.png') if t is not None and (convert.WORK / f'metal-{t:05d}.png').exists() else None
    glb.write(str(out), [dict(name='metal-chao', texset='metal', surfaces=M)], tex, dict(nodes=nodes, ibm=ibm, root=root, animations=anims,
              # gameUnit: one game unit in Chao-model units, so speeds convert to the Chao's world.
              extras=dict(gameUnit=float(total), clips={k: dict(mode=r['mode'], speed=r['speed']) for k, r in table.items()},
                          physics=physics(convert.GAME / 'sonic2app.exe'))))
    return out

if __name__ == '__main__':
    import sys
    if len(sys.argv) > 1: convert.GD = Path(sys.argv[1]) / 'resource/gd_PC'
    print('Wrote', build(convert.OUT / 'metal-chao-rigged.glb'))
