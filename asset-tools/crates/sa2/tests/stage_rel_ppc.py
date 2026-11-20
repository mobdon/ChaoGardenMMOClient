"""Load a GameCube stage REL (GSNE8P) into a dol_ppc Machine.

The REL comes from the user's own disc (for example stg09D.rel extracted next
to main.dol under .local); it is never committed. Its sections are copied to
a fixed base, the BSS section is zeroed, and every relocation against the REL
itself or against main.dol (module 0) is applied, so its functions run under
Unicorn with their real main.dol callees.

    from stage_rel_ppc import load_rel
    rel = load_rel(m, path)
    m.call(rel.addr(1, 0x51034), r3=task)
"""
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gekko_tables  # noqa: E402

REL_BASE = 0x80C00000


class LoadedRel:
    def __init__(self, data, bases):
        self.data = data
        self.bases = bases

    def addr(self, sec, off):
        return self.bases[sec] + off

    def section(self, sec):
        n, info = struct.unpack_from('>II', self.data, 0xC)
        off, size = struct.unpack_from('>II', self.data, info + 8 * sec)
        return self.data[off & ~1:(off & ~1) + size] if off else bytes(size)


def load_rel(m, path, base=REL_BASE):
    d = Path(path).read_bytes()
    module, = struct.unpack_from('>I', d, 0)
    n, info = struct.unpack_from('>II', d, 0xC)
    bases, at = {}, base
    for i in range(n):
        off, size = struct.unpack_from('>II', d, info + 8 * i)
        if not size:
            continue
        at = (at + 0x1F) & ~0x1F
        bases[i] = at
        m.u.mem_write(at, d[off & ~1:(off & ~1) + size] if off & ~1 else bytes(size))
        at += size
    imp, impsize = struct.unpack_from('>II', d, 0x28)
    for k in range(impsize // 8):
        mod, roff = struct.unpack_from('>II', d, imp + 8 * k)
        if mod not in (0, module):
            continue
        p, sec, cur = roff, 0, 0
        while True:
            o, t, s, a = struct.unpack_from('>HBBI', d, p)
            p += 8
            cur += o
            if t == 203:
                break
            if t == 202:
                sec, cur = s, 0
                continue
            if t == 201:
                continue
            val = a if mod == 0 else bases[s] + a
            where = bases[sec] + cur
            if t == 1:
                m.put(where, 'I', val & 0xffffffff)
            elif t == 4:
                m.put(where, 'H', val & 0xffff)
            elif t == 5:
                m.put(where, 'H', (val >> 16) & 0xffff)
            elif t == 6:
                m.put(where, 'H', ((val + 0x8000) >> 16) & 0xffff)
            elif t == 10:
                ins = m.get(where, 'I')
                m.put(where, 'I', (ins & 0xFC000003) | ((val - where) & 0x03FFFFFC))
            elif t == 11:
                ins = m.get(where, 'I')
                m.put(where, 'I', (ins & 0xFFFF0003) | ((val - where) & 0xFFFC))
            else:
                raise ValueError(f'relocation type {t} at {sec}:{cur:#x}')
    # Gekko's frsqrte estimate in the REL's code too, as dol_ppc does for
    # main.dol's (when the local tables exist).
    if gekko_tables.load() is not None:
        off, size = struct.unpack_from('>II', d, info + 8)
        for k in range(0, size, 4):
            ins = struct.unpack_from('>I', d, (off & ~1) + k)[0]
            if ins >> 26 == 63 and (ins >> 1) & 0x1F == 26:
                gekko_tables.install_frsqrte(m.u, bases[1] + k)
    return LoadedRel(d, bases)
