"""Oracle for sa2-runtime's PAD path: main.dol's fn_80119A04 (PADClamp runs
natively; PADRead is replaced by recorded PADStatus values) and fn_800226E0
with fn_80022048 under Unicorn. PADReset and the routines fn_800226E0 calls
after the copy are stubbed and logged. Prints JSON: the initial state, then
the events and full PAD state after every op.

Without SA2_MAIN_DOL it prints nothing and exits 0.
"""
import json, os, struct, sys
from pathlib import Path

if not os.environ.get('SA2_MAIN_DOL'):
    print('skipped: SA2_MAIN_DOL is not set', file=sys.stderr)
    sys.exit(0)

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'sa2' / 'tests'))
from dol_ppc import Dol  # noqa: E402

READ, TICK, PADREAD, PADRESET = 0x80119A04, 0x800226E0, 0x800F4780, 0x800F4368
LOGGED = {0x80022BC4: 0, 0x80021E68: 0, 0x80021D0C: 0, 0x80022A94: 1}
G, RECORDS, IDLE, PORTCFG = 0x801CC168, 0x80357E60, 0x801950E8, 0x8019511C
PREV, CUR, POINTERS, CONNECTED, INDEX = 0x80357F30, 0x80357F60, 0x801E55D8, 0x801E5368, 0x801E5358
COPIES, STICKS, PUSHED = 0x801E5438, 0x801E53B8, 0x803AD874
CAMPTR, VIEWPORTS, CAMTABLE = 0x803ADB04, 0x803ADAD4, 0x80175378
STATUS, CAMS = 0x81590000, 0x81590100


def pack(p):
    return struct.pack('>HbbbbBBBBbx', *p)


class PadOracle:
    def __init__(self, dol):
        self.m = m = dol.machine(boot=False)
        self.events, self.pads = [], [[0] * 9 + [-1]] * 4
        m.stub(PADREAD, self.padread)
        m.stub(PADRESET, lambda m: self.events.append(['engine', PADRESET, [m.gpr(3)]]))
        for a, n in LOGGED.items():
            m.stub(a, self.logger(a, n))
        m.u.mem_write(G, bytes(0xF0))
        for p in range(4):
            m.put(RECORDS + 0x34 * p + 0x30, 'I', STATUS + 4 * p)
        for i in range(8):
            m.put(CAMTABLE + 4 * i, 'I', CAMS + 0x20 * i)

    def logger(self, a, n):
        def f(m):
            self.events.append(['engine', a, [m.gpr(3 + k) for k in range(n)]])
        return f

    def padread(self, m):
        m.u.mem_write(m.gpr(3), b''.join(pack(p) for p in self.pads))

    def hexmem(self, a, n):
        return bytes(self.m.u.mem_read(a, n)).hex()

    def words(self, a, n, f='I'):
        return [self.m.get(a + 4 * i, f) for i in range(n)]

    def init(self):
        return {'records': self.hexmem(RECORDS, 0x34 * 4), 'idle': self.hexmem(IDLE, 0x34),
                'port_config': self.words(PORTCFG, 4)}

    def state(self):
        return {
            'prev': self.hexmem(PREV, 0x30), 'cur': self.hexmem(CUR, 0x30),
            'records': self.hexmem(RECORDS, 0x34 * 4), 'status': self.words(STATUS, 4),
            'pointers': self.words(POINTERS, 4), 'connected': self.words(CONNECTED, 4),
            'port_index': self.words(INDEX, 4, 'i'), 'copies': self.hexmem(COPIES, 0x34 * 8),
            'sticks': self.hexmem(STICKS, 0x80), 'pushed': self.hexmem(PUSHED, 8),
        }

    def run(self, op):
        m, k = self.m, op['op']
        if k == 'state':
            for off, v in op.get('g', {}).items():
                m.put(G + int(off), 'B', v)
            if 'camera_active' in op:
                m.put(CAMPTR, 'I', 0x81590400 if op['camera_active'] else 0)
            if 'viewports' in op:
                m.put(VIEWPORTS, 'i', op['viewports'])
            for i, a in enumerate(op.get('camera_angles', [])):
                m.put(CAMS + 0x20 * i + 0x10, 'I', a)
            for i, c in enumerate(op.get('port_config', [])):
                m.put(PORTCFG + 4 * i, 'I', c)
        elif k == 'read':
            self.pads = op['pads']
            m.call(READ)
        elif k == 'tick':
            m.call(TICK, count=5_000_000)
        else:
            raise ValueError(k)


def pad(button=0, stick=(0, 0), sub=(0, 0), trig=(0, 0), err=0):
    return [button, stick[0], stick[1], sub[0], sub[1], trig[0], trig[1], 0, 0, err]


NC = pad(err=-1)


def scenarios():
    rt = lambda pads: [{'op': 'read', 'pads': pads}, {'op': 'tick'}]
    s = []
    ops = [{'op': 'state', 'g': {'62': 1, '63': 1, '64': 1}}]
    ops += rt([pad(0x100), pad(0x1200), NC, NC])
    ops += rt([pad(0x500, (127, 0)), pad(0, (-128, -128)), NC, NC])
    ops += rt([pad(0x400, (10, -10), (127, -128), (255, 200)), pad(0, (20, 0)), NC, NC])
    ops += rt([pad(0, (15, -15), (-40, 40), (40, 10)), pad(0, (0, 80)), NC, NC])
    ops += rt([pad(0x0F, (-80, 0)), pad(0x70, (0, -90)), NC, NC])
    ops += rt([pad(0x0F, (33, 47), err=-2), pad(0x70, (-60, 50)), NC, pad(0x100)])
    ops += rt([pad(0x1F7F, (-57, -66)), pad(0, (72, 72)), pad(0x100), NC])
    ops += rt([pad(0, (127, 127)), pad(0x1F7F, (-128, 127)), NC, NC])
    s.append({'name': 'edges_two_ports', 'ops': ops})
    ops = [{'op': 'state', 'g': {'62': 1, '63': 0, '64': 1}, 'port_config': [6, 0, 7, 30]}]
    ops += rt([pad(0x100, (60, 0)), pad(0x200, (0, 60)), pad(0x400), NC])
    ops += rt([pad(0x100, (60, 0)), NC, pad(0x400), pad(0x800)])
    ops += [{'op': 'state', 'g': {'62': 0}}] + rt([pad(0x100, (60, 10)), pad(0x200), NC, NC])
    ops += [{'op': 'state', 'g': {'62': 1, '63': 1}, 'port_config': [18, 12, 6, 0]}]
    ops += rt([NC, NC, NC, NC]) + rt([NC, NC, pad(0x100, (90, -90)), pad(0x200)])
    s.append({'name': 'flags_and_ports', 'ops': ops})
    ops = [{'op': 'state', 'g': {'62': 1, '63': 1, '64': 1}, 'camera_active': 1, 'viewports': 1,
            'camera_angles': [0x1000, 0x7FF0, 0xC000, 0, 0, 0, 0, 0]}]
    for v in (20, 30, 40, 45, 50, 40, 60, 90, 20):
        ops += rt([pad(0, (v, v // 2)), pad(0, (-v, v)), NC, NC])
    ops += [{'op': 'state', 'viewports': 2}] + rt([pad(0, (-50, -50)), pad(0, (50, -20)), NC, NC])
    ops += [{'op': 'state', 'camera_active': 0}] + rt([pad(0, (0, -127)), pad(0, (-127, 0)), NC, NC])
    s.append({'name': 'camera_and_magnitude', 'ops': ops})
    return s


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    out = []
    for sc in scenarios():
        o = PadOracle(dol)
        init = o.init()
        steps = []
        for op in sc['ops']:
            o.events = []
            o.run(op)
            steps.append({'events': o.events, 'state': o.state()})
        out.append({'name': sc['name'], 'init': init, 'ops': sc['ops'], 'steps': steps})
    json.dump({'scenarios': out}, sys.stdout)


main()
