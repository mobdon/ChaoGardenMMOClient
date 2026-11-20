"""Load a GameCube REL stage module into a dol_ppc Machine, relocated.

Places every section (and .bss) at `base` upwards, then applies the module's
own and main.dol (module 0) relocations. Returns the section addresses.
Supported kinds: ADDR32 (1), ADDR16_LO (4), ADDR16_HI (5), ADDR16_HA (6),
REL24 (10), plus the NOP / SECTION / END markers (201, 202, 203).
`others` maps the id of an already loaded module (for example ChaoMain) to the
section addresses load_rel returned for it, so imports from it resolve too. Imports from module ids in `skip` are left
unrelocated (for modules the caller never runs, such as ChaoMotionsD).
"""
import struct


def load_rel(m, data, base, others=None, skip=()):
    w = lambda o: struct.unpack_from(">I", data, o)[0]
    module = w(0)
    count, info = w(12), w(16)
    bss_size = w(0x20)
    imp, imp_size = w(0x28), w(0x2C)
    addrs = {}
    cursor = base
    for s in range(count):
        off, size = w(info + 8 * s), w(info + 8 * s + 4)
        if size == 0:
            continue
        cursor = (cursor + 0x1F) & ~0x1F
        addrs[s] = cursor
        if off & ~1:
            m.u.mem_write(cursor, data[off & ~1:(off & ~1) + size])
        else:
            m.u.mem_write(cursor, bytes(size))  # .bss
        cursor += size
    if bss_size and not any(w(info + 8 * s) & ~1 == 0 and w(info + 8 * s + 4) for s in range(count)):
        cursor = (cursor + 0x1F) & ~0x1F
        m.u.mem_write(cursor, bytes(bss_size))
    for k in range(imp_size // 8):
        mod, at = struct.unpack_from(">II", data, imp + 8 * k)
        others = others or {}
        if mod in skip:
            continue
        if mod not in (0, module) and mod not in others:
            raise ValueError(f"import from module {mod}")
        target = addrs if mod == module else others.get(mod)
        section, offset = None, 0
        while True:
            delta, kind, s, addend = struct.unpack_from(">HBBI", data, at)
            at += 8
            if kind == 203:
                break
            offset += delta
            if kind == 202:
                section, offset = s, 0
                continue
            if kind == 201:
                continue
            value = addend if mod == 0 else target[s] + addend
            where = addrs[section] + offset
            if kind == 1:
                m.u.mem_write(where, struct.pack(">I", value & 0xFFFFFFFF))
            elif kind in (4, 5, 6):
                half = value & 0xFFFF if kind == 4 else (value >> 16) & 0xFFFF if kind == 5 else ((value + 0x8000) >> 16) & 0xFFFF
                m.u.mem_write(where, struct.pack(">H", half))
            elif kind == 10:
                ins = struct.unpack(">I", bytes(m.u.mem_read(where, 4)))[0]
                dist = value - where
                assert -(1 << 25) <= dist < (1 << 25)
                m.u.mem_write(where, struct.pack(">I", (ins & 0xFC000003) | (dist & 0x3FFFFFC)))
            else:
                raise ValueError(f"relocation kind {kind}")
    return addrs
