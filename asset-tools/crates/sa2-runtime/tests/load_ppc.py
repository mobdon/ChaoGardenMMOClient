"""Oracle for sa2-runtime's player load and camera task creation: main.dol's
fn_8001DEA8 (with the Mech Eggman loader fn_800B5D2C, fn_8004F838 and
fn_800B5630) and fn_800C4458 under Unicorn.

Task routines run natively. Other character loaders, CCL_Init, the motion and
model loaders, GX texture setup and the camera mode routines are stubbed and
logged; pointer arguments are logged as player-work offsets (0xFF000000 | off)
or allocation numbers (0xFE000000 | n). Pointer words in player work and the
player tables are reported as symbols. Prints JSON.

Without SA2_MAIN_DOL it prints nothing and exits 0.
"""
import json, os, struct, sys
from pathlib import Path

if not os.environ.get('SA2_MAIN_DOL'):
    print('skipped: SA2_MAIN_DOL is not set', file=sys.stderr)
    sys.exit(0)

sys.path.insert(0, str(Path(__file__).resolve().parent))
from task_system_ppc import Oracle, Dol, WORK  # noqa: E402

LOAD_PLAYERS, CAMERA_INIT, CET = 0x8001DEA8, 0x800C4458, 0x80071E0C
G, STAGE, CHARS, UPGRADES, PHYSICS = 0x801CC168, 0x803AD820, 0x803AD828, 0x801E50B4, 0x80196578
TABLES = {'pwp': 0x801E7728, 'mwp': 0x801E7748, 'twp': 0x801E7768, 'tp': 0x801E7788}
CAM = {'task': 0x803ADAF8, 'viewports': 0x803ADAD4, 'index': 0x803ADAD0, 'adb1c': 0x803ADB1C,
       'adb00': 0x803ADB00, 'adb04': 0x803ADB04, 'adafc': 0x803ADAFC}
CAM_TABLE, CAM_BLOCKS, CAM_STEP = 0x80175378, 0x801FF424, 0x24D8
COLL = 0x81595000
TEX_SIZE = 0x20000
ARG_PWP, ARG_ALLOC = 0xFF000000, 0xFE000000
LOGGED = {  # address: argument kinds ('i' integer, 'p' player-work pointer, 'a' allocation, 't' task)
    0x800874C0: 'i', 0x80086E3C: 'i', 0x800A9930: 'i', 0x800D92E4: 'i', 0x800A2360: 'i',
    0x800A1F00: 'i', 0x800B5AC0: 'i', 0x800B56E8: 'i', 0x8001D74C: 'i',
    0x800645F8: 'i', 0x80010CC4: 'i', 0x8003D8D0: 'p',
    0x8010334C: 'paiiiiii', 0x80103608: 'piiiii',
    0x800C34C4: '', 0x800C5914: '', 0x800C6F70: '', 0x800C91C4: '', 0x800C36A4: 'i',
    0x800C3A94: 'iii', 0x800C3C38: '', 0x800C76F4: 'i',
}
VALUES = (0x80072548, 0x80050210, 0x80050094)


class LoadOracle(Oracle):
    def __init__(self, dol, sc):
        super().__init__(dol, sc)
        m = self.m
        self.others = []  # (address, size)
        self.pwp = None
        self.value = 0
        for a, kinds in LOGGED.items():
            m.stub(a, self.logger(a, kinds))
        for a in VALUES:
            m.stub(a, self.valued(a))
        m.stub(0x80103128, self.tex_size)
        m.stub(0x8000F8B8, self.ccl_init)
        # Destructors the loaders install are not ported: logged as calls, not run.
        for a in (0x800B245C, 0x800C3D90):
            m.stub(a, lambda m, a=a: self.events.append(['call', a, self.ids[m.gpr(3)]]))
        m.u.mem_write(G, bytes(0xF0))

    def calloc(self, m):
        n = m.gpr(3) * m.gpr(4)
        a = self.alloc(n)
        lr = m.u.reg_read(__import__('unicorn').ppc_const.UC_PPC_REG_LR)
        if CET <= lr < CET + 0x120:
            self.work[a] = (self.ids[self.last_task], WORK[n])
        else:
            self.others.append((a, n))
            if n == 0x454:
                self.pwp = a
        m.gpr(3, a)

    def free(self, m):
        a = m.gpr(3)
        if any(x == a for x, n in self.others):
            assert a not in self.freed, hex(a)
            self.freed.add(a)
            owner = next(i for t, i in self.ids.items() if m.get(t + 0x40, 'I') == a)
            self.events.append(['free_work', owner, 0x40])
            return
        super().free(m)

    def arg(self, kind, v):
        if kind == 'p':
            return ARG_PWP | (v - self.pwp)
        if kind == 'a':
            return ARG_ALLOC | next(i for i, (a, n) in enumerate(self.others) if a == v)
        if kind == 't':
            return self.ids[v]
        return v

    def logger(self, addr, kinds):
        def f(m):
            self.events.append(['engine', addr, [self.arg(k, m.gpr(3 + i)) for i, k in enumerate(kinds)]])
        return f

    def valued(self, addr):
        def f(m):
            self.events.append(['engine', addr, [m.gpr(3)]])
            m.gpr(3, 0xC0000000 + self.value)
            self.value += 1
        return f

    def tex_size(self, m):
        self.events.append(['engine', 0x80103128, [m.gpr(3 + i) for i in range(5)]])
        m.gpr(3, TEX_SIZE)

    def ccl_init(self, m):
        t = m.gpr(3)
        i = self.ids[t]
        self.events.append(['engine', 0x8000F8B8, [i, m.gpr(4), m.gpr(5), m.gpr(6)]])
        twp = m.get(t + 0x34, 'I')
        m.put(twp + 0x2C, 'I', COLL + 0x40 * i)
        m.put(COLL + 0x40 * i + 0xC, 'I', COLL + 0x40 * i + 0x20)

    def sym(self, v):
        if v == 0:
            return None
        if v in self.ids:
            return 'task%d' % self.ids[v]
        if v in self.work:
            i, off = self.work[v]
            return {0x34: 'twp', 0x38: 'mwp', 0x3C: 'fwp', 0x40: 'awp'}[off] + str(i)
        for n, (a, size) in enumerate(self.others):
            if a <= v < a + size:
                return 'a%d' % n + ('' if v == a else '+0x%x' % (v - a))
        return '%08x' % v

    def load_state(self):
        m = self.m
        out = {name: [self.sym(m.get(a + 4 * p, 'I')) for p in range(4)] for name, a in TABLES.items()}
        works = {}
        radius = []
        for t, i in self.ids.items():
            if t in self.freed:
                continue
            w = m.get(t + 0x40, 'I')
            if w and any(a == w and n == 0x454 for a, n in self.others):
                words = []
                for k in range(0, 0x454, 4):
                    v = m.get(w + k, 'I')
                    s = self.sym(v) if 0x81000000 <= v < 0x81500000 else '%08x' % v
                    words.append(s)
                works[str(i)] = words
                radius.append([i, m.get(COLL + 0x40 * i + 0x20 + 0x14, 'I')])
        out['works'] = works
        out['radius'] = radius
        out['allocs'] = [n for a, n in self.others]
        cam = {k: m.get(a, 'I') for k, a in CAM.items()}
        cam['task'] = None if cam['task'] == 0 else self.ids[cam['task']]
        cam['viewports'] = m.get(CAM['viewports'], 'i')
        cam['index'] = m.get(CAM['index'], 'i')
        cam['table'] = [m.get(CAM_TABLE + 4 * i, 'I') for i in range(4)]
        cam['blocks'] = [[m.get(CAM_BLOCKS + CAM_STEP * i + 4 * k, 'I') for k in range(3)]
                         for i in range(4)]
        out['camera'] = cam
        return out

    def run(self, op):
        m, k = self.m, op['op']
        if k == 'state':
            for off, v in op.get('g', {}).items():
                m.put(G + int(off), 'B', v)
            if 'stage' in op:
                m.put(STAGE, 'h', op['stage'])
            for i, c in enumerate(op.get('chars', [])):
                m.put(CHARS + 4 * i, 'i', c)
            for i, b in enumerate(op.get('upgrades', [])):
                m.put(UPGRADES + i, 'B', b)
            if 'adb00' in op:
                m.put(CAM['adb00'], 'I', op['adb00'])
        elif k == 'players':
            m.call(LOAD_PLAYERS, count=5_000_000)
        elif k == 'camera':
            m.call(CAMERA_INIT, r3=op['count'])
        else:
            super().run(op)


def scenarios():
    up = [0] * 0x1D
    up_mech = list(up)
    for i in (0x14, 0x16, 0x17):
        up_mech[i] = 1
    up_all = [1] * 0x1D
    return [
        {'name': 'iron_gate_one_player', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up},
            {'op': 'camera', 'count': 1}, {'op': 'players'}]},
        {'name': 'iron_gate_upgrades', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up_mech},
            {'op': 'players'}, {'op': 'camera', 'count': 1}]},
        {'name': 'special_stage_and_demo', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 30, 'chars': [7, 0], 'upgrades': up_all, 'g': {'0': 1}},
            {'op': 'players'},
            {'op': 'state', 'stage': 61, 'adb00': 1}, {'op': 'camera', 'count': 2},
            {'op': 'camera', 'count': 3},
            {'op': 'state', 'stage': 60, 'adb00': 0}, {'op': 'camera', 'count': 1},
            {'op': 'state', 'stage': 59}, {'op': 'camera', 'count': 4}]},
        {'name': 'two_player_paths', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 0x1D, 'chars': [0, 7], 'g': {'30': 1, '84': 1},
             'upgrades': up_mech},
            {'op': 'players'},
            {'op': 'state', 'chars': [7, 7], 'g': {'86': 1, '83': 1}}, {'op': 'players'},
            {'op': 'state', 'chars': [3, 9], 'g': {'83': 2}}, {'op': 'players'},
            {'op': 'state', 'stage': 0x14, 'chars': [5, 7], 'g': {'30': 1, '84': 0, '86': 0}},
            {'op': 'players'},
            {'op': 'destroy_all'}]},
    ]


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    m0 = dol.machine(boot=False)
    physics = [bytes(m0.u.mem_read(PHYSICS + 0x84 * r, 0x84)).hex() for r in range(17)]
    out = []
    for sc in scenarios():
        o = LoadOracle(dol, sc)
        steps = []
        for op in sc['ops']:
            o.events = []
            o.run(op)
            steps.append({'events': o.events, 'state': o.state(), 'load': o.load_state()})
        out.append({'name': sc['name'], 'cbs': {}, 'ops': sc['ops'], 'steps': steps})
    json.dump({'physics': physics, 'tex_size': TEX_SIZE, 'coll': COLL, 'scenarios': out},
              sys.stdout)


main()
