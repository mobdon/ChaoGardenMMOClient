"""Shared set-up for the stage_objects oracles (stage_objects_ppc.py and
so_*_ppc.py): main.dol in the Unicorn harness with stg42D.rel loaded and
relocated, an event recorder for stubbed engine routines, and float
helpers. Needs SA2_MAIN_DOL and SA2_GC_REL_DIR (local files from the user's
own disc, never committed).

    from so_harness import *   # m, rel, S1, S4, S5, S6, ev, events, ret, fb, bf, f32, rng
"""
import os, random, struct, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from dol_ppc import Dol  # noqa: E402
from stage_rel_ppc import load_rel  # noqa: E402

fb = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
bf = lambda b: struct.unpack('>f', struct.pack('>I', b))[0]
f32 = lambda x: bf(fb(x))

dol = Dol(os.environ["SA2_MAIN_DOL"])
m = dol.machine()
rel = load_rel(m, Path(os.environ["SA2_GC_REL_DIR"]) / "stg42D.rel")
S1 = lambda off: rel.addr(1, off)
S4 = lambda off: rel.addr(4, off)
S5 = lambda off: rel.addr(5, off)
S6 = lambda off: rel.addr(6, off)

# Free scratch RAM for tasks and work blocks (above the REL and its BSS).
SCRATCH = 0x81200000
events = []
rng = random.Random(42)


def ev(*e):
    events.append(list(e))


def ret(fn):
    """A stub returning fn(machine) in r3."""
    def stub(mm):
        mm.gpr(3, int(fn(mm)) & 0xffffffff)
    return stub


def ret_f(fn):
    """A stub returning fn(machine) in f1."""
    def stub(mm):
        mm.fpr(1, fn(mm))
    return stub


def clear(addr, n):
    m.u.mem_write(addr, bytes(n))


def rnd_f(lo=-500.0, hi=500.0):
    if rng.random() < 0.1:
        return rng.choice([0.0, -0.0, 1.0, -10.0, 0.5, 1e30, -1e30])
    return f32(rng.uniform(lo, hi))
