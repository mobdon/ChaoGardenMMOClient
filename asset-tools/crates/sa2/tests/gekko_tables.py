"""Gekko `frsqrte` and `fres` estimates for the main.dol harness.

Unicorn (QEMU) and the paired-single interpreter in dol_ppc.py compute
`frsqrte` and `fres` exactly. The GameCube's Gekko returns table estimates.
Those tables are transcribed from Dolphin's GPL source
(Source/Core/Common/FloatUtils.cpp), so they are not in this repository:
they are read from a local, git-ignored JSON file, the same file the Rust
crate reads (`msl_math::gekko_tables`):

    .local/research/gekko-estimate-tables.json   (override: SA2_GEKKO_TABLES)
    {"frsqrte": [[base, slope], ... 32], "fres": [[base, slope], ... 32]}

`load()` returns the tables, or None when the file is absent. Anything that
needs bit-exact estimates calls `require()`, which skips the calling oracle
script (exit status 0, "SKIP" on stderr) when the file is missing.

To write the local file from this repository's history (the tables were
committed before they were moved out):

    python tests/gekko_tables.py --extract-from-git fc5e0a3
"""
import json, os, re, struct, subprocess, sys
from pathlib import Path

REL_PATH = Path(".local/research/gekko-estimate-tables.json")
ENV = "SA2_GEKKO_TABLES"
QNAN = 0x7FF8000000000000
F32_MAX = 0x47EFFFFFE0000000
_cache = []


def path():
    """The table file in use, or the default location if none is found."""
    if os.environ.get(ENV):
        return Path(os.environ[ENV])
    for start in (Path.cwd(), Path(__file__).resolve().parent):
        for d in (start, *start.parents):
            if (d / REL_PATH).is_file():
                return d / REL_PATH
    return Path(__file__).resolve().parents[4] / REL_PATH


def _table(v, key):
    rows = v[key]
    assert len(rows) == 32 and all(len(r) == 2 for r in rows), f"bad {key} table"
    return [(int(a), int(b)) for a, b in rows]


def load():
    """{'frsqrte': [...], 'fres': [...]} from the local file, or None."""
    if not _cache:
        p = path()
        if p.is_file():
            v = json.loads(p.read_text())
            _cache.append({"frsqrte": _table(v, "frsqrte"), "fres": _table(v, "fres")})
        else:
            _cache.append(None)
    return _cache[0]


def require():
    t = load()
    if t is None:
        print(f"SKIP: Gekko estimate tables not found at {path()} (set {ENV})", file=sys.stderr)
        sys.exit(0)
    return t


def frsqrte_bits(bits):
    """Gekko frsqrte on a double's bit pattern; returns the result's bits."""
    table = require()["frsqrte"]
    mantissa = bits & ((1 << 52) - 1)
    sign = bits & (1 << 63)
    exponent = bits & (0x7FF << 52)
    if mantissa == 0 and exponent == 0:
        return 0xFFF0000000000000 if sign else 0x7FF0000000000000
    if exponent == 0x7FF << 52:
        if mantissa == 0:
            return QNAN if sign else 0
        return bits | 0x0008000000000000
    if sign:
        return QNAN
    if exponent == 0:
        while True:
            exponent -= 1 << 52
            mantissa <<= 1
            if mantissa & (1 << 52):
                break
        mantissa &= (1 << 52) - 1
        exponent += 1 << 52
    exponent_lsb = exponent & (1 << 52)
    # C++ signed division truncates toward zero.
    delta = exponent - (0x3FE << 52)
    half = abs(delta) // 2 * (1 if delta >= 0 else -1)
    exponent = ((0x3FF << 52) - half) & (0x7FF << 52)
    i = (exponent_lsb | mantissa) >> 37
    base, dec = table[i // 2048]
    return (sign | exponent | ((base + dec * (i % 2048)) << 26)) & 0xFFFFFFFFFFFFFFFF


def fres_bits(bits):
    """Gekko fres on a double's bit pattern; returns the result's bits."""
    table = require()["fres"]
    mantissa = bits & ((1 << 52) - 1)
    sign = bits & (1 << 63)
    exponent = bits & (0x7FF << 52)
    if mantissa == 0 and exponent == 0:
        return sign | (0x7FF << 52)
    if exponent == 0x7FF << 52:
        if mantissa == 0:
            return sign
        return bits | 0x0008000000000000
    if exponent < 895 << 52:
        return sign | F32_MAX
    if exponent >= 1149 << 52:
        return sign
    exponent = (0x7FD << 52) - exponent
    i = mantissa >> 37
    base, dec = table[i // 1024]
    return sign | exponent | ((base - (dec * (i % 1024) + 1) // 2) << 29)


def _bits(x):
    return struct.unpack(">Q", struct.pack(">d", x))[0]


def _float(b):
    return struct.unpack(">d", struct.pack(">Q", b))[0]


def frsqrte(x):
    return _float(frsqrte_bits(_bits(x)))


def fres(x):
    return _float(fres_bits(_bits(x)))


def install_frsqrte(cpu, address):
    """Replace the `frsqrte` instruction at `address` with the Gekko estimate."""
    from unicorn import UC_HOOK_CODE
    from unicorn import ppc_const as R

    require()
    ins = struct.unpack(">I", cpu.mem_read(address, 4))[0]
    assert ins >> 26 == 63 and (ins >> 1) & 0x1F == 26, f"no frsqrte at {address:#x}"
    d, b = (ins >> 21) & 31, (ins >> 11) & 31

    def hook(u, pc, size, _):
        src = u.reg_read(getattr(R, f"UC_PPC_REG_FPR{b}"))
        u.reg_write(getattr(R, f"UC_PPC_REG_FPR{d}"), frsqrte_bits(src))
        u.reg_write(R.UC_PPC_REG_PC, pc + 4)

    cpu.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)


def install(m, dol_ppc):
    """Use the Gekko estimate for every `fres` machine `m` runs: each `fres`
    in the DOL's text under Unicorn, and in dol_ppc's interpreter."""
    from unicorn import UC_HOOK_CODE
    from unicorn import ppc_const as R

    require()
    for a, o, z in m.dol.sections:
        if a >= 0x80173140:  # text sections only
            continue
        for k in range(0, z, 4):
            ins = struct.unpack_from(">I", m.dol.data, o + k)[0]
            if ins >> 26 == 59 and (ins >> 1) & 0x1F == 24:
                d, b = (ins >> 21) & 31, (ins >> 11) & 31

                def hook(u, pc, size, _, d=d, b=b):
                    src = u.reg_read(getattr(R, f"UC_PPC_REG_FPR{b}"))
                    u.reg_write(getattr(R, f"UC_PPC_REG_FPR{d}"), fres_bits(src))
                    u.reg_write(R.UC_PPC_REG_PC, pc + 4)

                m.u.hook_add(UC_HOOK_CODE, hook, begin=a + k, end=a + k)
    plain = dol_ppc.Interp.fp_single

    def fp_single(self, ins):
        if (ins >> 1) & 31 == 24:
            d, b = (ins >> 21) & 31, (ins >> 11) & 31
            v = fres(self.p0[b])
            self.p0[d] = v
            self.p1[d] = v
            return
        plain(self, ins)

    dol_ppc.Interp.fp_single = fp_single


def extract_from_git(rev):
    """Read both tables from msl_math.rs as committed at `rev`."""
    src = subprocess.run(
        ["git", "show", f"{rev}:asset-tools/crates/sa2/src/msl_math.rs"],
        check=True, capture_output=True, text=True,
    ).stdout
    out = {}
    for key, name in (("frsqrte", "FRSQRTE_EXPECTED"), ("fres", "FRES_EXPECTED")):
        body = re.search(name + r": \[\(i64, i64\); 32\] = \[(.*?)\];", src, re.S).group(1)
        rows = re.findall(r"\((-?0x[0-9A-Fa-f]+), (-?0x[0-9A-Fa-f]+)\)", body)
        out[key] = [[int(a, 16), int(b, 16)] for a, b in rows]
        assert len(out[key]) == 32, f"{name}: {len(out[key])} rows at {rev}"
    return out


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--extract-from-git":
        tables = extract_from_git(sys.argv[2])
        p = path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(tables) + "\n")
        print(f"wrote {p}")
    else:
        print(__doc__)
