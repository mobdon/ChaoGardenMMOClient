"""Run stg42D.rel's collision primitives (SPHERE, CCYL, CCUBE, CWALL,
CCIRCLE) under the main.dol harness and print JSON (every float as its bit
pattern). tests/stage_objects.rs replays each case with `stage_objects` and
compares. Usage: python stage_objects_ppc.py colli_prim
The engine routines the objects call are stubbed and recorded as events;
everything in the stage REL runs as compiled.
"""
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
from so_harness import *  # noqa: E402,F401,F403

T, TWP, CWP, INFO = SCRATCH, SCRATCH + 0x100, SCRATCH + 0x200, SCRATCH + 0x400
frame = {}
which = sys.argv[1]
out = {'object': which, 'cases': []}


# --- SPHERE, CCYL, CCUBE, CWALL, CCIRCLE ------------------------------------
def colli_prim():
    def calc(mm):
        assert mm.gpr(3) == CWP
        ev('calc', [m.get(INFO + 0x14 + 4 * k, 'I') for k in range(3)])
    m.stub(0x800655F4, ret(lambda mm: (ev('range_out'), frame['out'])[1]))
    m.stub(0x80065498, ret(lambda mm: (ev('range_out_r', fb(f32(mm.fpr(1)))), frame['out'])[1]))
    m.stub(0x8000F8B8, lambda mm: (ev('init', mm.gpr(4) - S5(0), mm.gpr(5), mm.gpr(6)), mm.gpr(3, 1)))
    m.stub(0x800068E4, calc)
    m.stub(0x8000F4EC, lambda mm: ev('entry'))
    for kind, exec_ in [('sphere', 0xBEF8), ('cylinder', 0xBF9C), ('cube', 0xC040), ('wall', 0xC0E4), ('circle', 0xC188)]:
        for _ in range(60):
            clear(T, 0x40); clear(TWP, 0x40); clear(CWP, 0x40); clear(INFO, 0x30)
            m.put(T + 0x34, 'I', TWP)
            m.put(TWP + 0x2C, 'I', CWP)
            m.put(CWP + 0xC, 'I', INFO)
            mode = rng.choice([0, 0, 1, 1, 2, -1, 127, -128, rng.randrange(-128, 128)])
            scale = [rnd_f() for _ in range(3)]
            m.put(TWP, 'b', mode)
            for k, s in enumerate(scale):
                m.put(TWP + 0x20 + 4 * k, 'f', s)
            frames = []
            for _ in range(4):
                frame['out'] = rng.random() < 0.2
                events.clear()
                m.call(S1(exec_), r3=T)
                frames.append({'out': frame['out'], 'events': list(events), 'mode': m.get(TWP, 'b')})
            out['cases'].append({'kind': kind, 'mode': mode, 'scale': [fb(s) for s in scale], 'frames': frames})


{'colli_prim': colli_prim}[which]()
print(json.dumps(out))
