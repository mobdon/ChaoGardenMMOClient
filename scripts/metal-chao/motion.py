# Big-endian character MTN bank (Steam *mtn.prs, decompressed): directory of (slot, parts, offset) and Ninja motions
# with position / rotation (BAMS) / scale keys, sampled the way sa2 pose.rs does (linear on stored Euler values).
import struct
class Bank:
    def __init__(s, data):
        s.d = data; s.rows = {}
        for i in range(0, 4097 * 8, 8):
            slot, parts, off = struct.unpack('>HHI', data[i:i + 8])
            if slot == 0xFFFF: break
            s.rows[slot] = (parts, off)
    def motion(s, slot):
        parts, off = s.rows[slot]; d = s.d
        base, frames, flags, interp = struct.unpack('>IIHH', d[off:off + 12])
        layout = {(7, 3): (('pos', '>I3f'), ('rot', '>I3i'), ('scl', '>I3f')), (3, 2): (('pos', '>I3f'), ('rot', '>I3i'))}.get((flags, interp))
        if layout is None: raise ValueError(f'motion {slot}: layout {flags:#x}/{interp} not handled')
        n_ = len(layout); out = []
        for p in range(parts):
            at = base + p * 8 * n_; w = struct.unpack(f'>{2 * n_}I', d[at:at + 8 * n_]); ch = {'scl': []}
            for k, (name, fmt) in enumerate(layout):
                ptr, n = w[k], w[n_ + k]
                ch[name] = [struct.unpack(fmt, d[ptr + q * 16:ptr + q * 16 + 16]) for q in range(n)]
            out.append(ch)
        return frames, out
def sample(keys, f):
    if not keys: return None
    if f <= keys[0][0]: return keys[0][1:]
    for a, b in zip(keys, keys[1:]):
        if a[0] <= f < b[0]:
            t = (f - a[0]) / (b[0] - a[0]); return tuple(x + (y - x) * t for x, y in zip(a[1:], b[1:]))
    return keys[-1][1:]
def frame_anim(parts, f):
    anim = {}
    for i, ch in enumerate(parts):
        a = {k: v for k in ('pos', 'rot', 'scl') if (v := sample(ch[k], f)) is not None}
        if 'rot' in a: a['rot'] = tuple(int(round(x)) for x in a['rot'])
        if a: anim[i] = a
    return anim
