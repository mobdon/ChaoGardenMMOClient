"""Extract main.dol from a GameCube disc image in Dolphin's RVZ format.

GameCube discs are not encrypted; RVZ is Dolphin's compressed container
(zstd groups, with runs of disc padding stored as lagged-Fibonacci seeds).
This reader handles GameCube RVZ files compressed with zstd, which is what
Dolphin writes by default, and only decompresses the groups it needs.

Usage: python extract_main_dol.py DISC.rvz OUT/main.dol
Requires the `zstandard` package. The output is checked against the SHA-1 of
the GSNE8P main.dol that tests/dol_ppc.py targets.
"""
import hashlib, struct, sys
import zstandard
K, J = 521, 32
class LFG:
    """Dolphin's lagged Fibonacci junk generator (as used by RVZ packing)."""
    def __init__(s, seed_bytes):
        b = list(struct.unpack('>17I', seed_bytes)) + [0] * (K - 17)
        for i in range(17, K):
            b[i] = ((b[i - 17] << 23) & 0xffffffff) ^ (b[i - 16] >> 9) ^ b[i - 1]
        s.b = [(x & 0xFF00FFFF) | ((x >> 2) & 0x00FF0000) for x in b]
        s.pos = 0
        for _ in range(4): s.forward()
    def forward(s):
        b = s.b
        for i in range(J): b[i] ^= b[i + K - J]
        for i in range(J, K): b[i] ^= b[i - J]
    def skip(s, n):
        s.pos += n
        while s.pos >= K * 4: s.forward(); s.pos -= K * 4
    def take(s, n):
        out = bytearray()
        while n:
            raw = struct.pack('>521I', *s.b)
            k = min(n, K * 4 - s.pos); out += raw[s.pos:s.pos + k]; s.pos += k; n -= k
            if s.pos == K * 4: s.forward(); s.pos = 0
        return bytes(out)
class RVZ:
    def __init__(s, path):
        s.f = open(path, 'rb'); h = s.f.read(0x124)
        assert h[:4] == b'RVZ\x01'
        s.iso_size, = struct.unpack_from('>Q', h, 0x24)
        disc_type, comp, lvl, s.chunk = struct.unpack_from('>IIiI', h, 0x48)
        assert (disc_type, comp) == (1, 5), (disc_type, comp)
        s.dhead = h[0x58:0xd8]
        n_raw, raw_off, raw_size, n_groups, group_off, group_size = struct.unpack_from('>IQIIQI', h, 0xfc)
        s.z = zstandard.ZstdDecompressor()
        raw = s.dec(s.read(raw_off, raw_size), 24 * n_raw)
        s.raws = [struct.unpack_from('>QQII', raw, 24 * i) for i in range(n_raw)]
        g = s.dec(s.read(group_off, group_size), 12 * n_groups)
        s.groups = [struct.unpack_from('>III', g, 12 * i) for i in range(n_groups)]
        s.cache = {}
    def read(s, off, n): s.f.seek(off); return s.f.read(n)
    def dec(s, b, n): return s.z.decompress(b, max_output_size=n)
    def group(s, gi, size, disc_off):
        if gi in s.cache: return s.cache[gi]
        off, dsize, packed = s.groups[gi]
        comp = dsize >> 31; dsize &= 0x7fffffff
        if dsize == 0: data = bytes(size)
        else:
            data = s.read(off << 2, dsize)
            if comp: data = s.dec(data, packed or size)
            if packed:
                out = bytearray(); i = 0
                while i < len(data):
                    n, = struct.unpack_from('>I', data, i); i += 4
                    if n & 0x80000000:
                        n &= 0x7fffffff
                        g = LFG(data[i:i + 68]); i += 68
                        g.skip((disc_off + len(out)) % 0x8000)
                        out += g.take(n)
                    else:
                        out += data[i:i + n]; i += n
                data = bytes(out)
        s.cache[gi] = data; return data
    def pread(s, off, n):
        out = bytearray()
        while n:
            if off < 0x80:
                k = min(n, 0x80 - off); out += s.dhead[off:off + k]; off += k; n -= k; continue
            for doff, dsize, gidx, ng in s.raws:
                skip = doff % 0x8000; start = doff - skip; end = doff + dsize
                if start <= off < end:
                    rel = off - start; gi = gidx + rel // s.chunk
                    gsize = min(s.chunk, end - (start + (rel // s.chunk) * s.chunk))
                    data = s.group(gi, gsize, start + (rel // s.chunk) * s.chunk)
                    within = rel % s.chunk; k = min(n, len(data) - within)
                    out += data[within:within + k]; off += k; n -= k; break
            else: raise ValueError(hex(off))
        return bytes(out)
if __name__ == '__main__':
    r = RVZ(sys.argv[1]); out = sys.argv[2]
    hdr = r.pread(0, 0x440)
    print(hdr[:6], hdr[0x20:0x40].split(b'\0')[0])
    dol_off, fst_off, fst_size = struct.unpack_from('>III', hdr, 0x420)
    dh = r.pread(dol_off, 0x100)
    offs = struct.unpack_from('>18I', dh, 0); sizes = struct.unpack_from('>18I', dh, 0x90)
    size = max(o + z for o, z in zip(offs, sizes) if z)
    dol = r.pread(dol_off, size)
    open(out, 'wb').write(dol)
    sha1 = hashlib.sha1(dol).hexdigest()
    print('main.dol at', hex(dol_off), 'size', hex(size), 'sha1', sha1)
    if sha1 != 'e4c50f32242a3cd36cdc8ae776661601a52130a2':
        print('warning: not the GSNE8P build the PowerPC tests target')
