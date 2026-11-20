"""Oracle for sa2-runtime's task system: main.dol's own task routines under
Unicorn on synthetic task lists.

Runs CreateElementalTask, CreateChildTask, FreeTask, FreeTaskC, DestroyTask,
clearBtp, the exec pass (fn_80071214), the display pass (fn_80070C54), the
+0x2C walk (fn_80071358) and the level purges (fn_80070ABC, fn_80070B30) of
GSNE8P main.dol. Callbacks are small PowerPC stubs written into free memory:
each one is logged on entry and then does its scripted actions by calling the
native routines. MAlloc, syCalloc and syFree are replaced by a bump allocator
that maps blocks to task ids; engine calls of the passes are stubbed and
logged. Prints JSON (scenarios, and the events and state after every op).

Without SA2_MAIN_DOL it prints nothing and exits 0. Nothing from the game is
written anywhere.
"""
import json, os, struct, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'sa2' / 'tests'))
from dol_ppc import Dol  # noqa: E402
from unicorn import UC_HOOK_CODE  # noqa: E402

CET, CCT, FREE, DESTROY, FREEC = 0x80071E0C, 0x800709F8, 0x80071AE0, 0x80071AF0, 0x8007098C
EXEC_PASS, DISPLAY_PASS, SHADOW_LEVEL = 0x80071214, 0x80070C54, 0x80071358
DESTROY_ALL, DESTROY_STAGE, CLEAR_BTP = 0x80070ABC, 0x80070B30, 0x80071F2C
MALLOC, CALLOC, SYFREE, CALC_POINT = 0x801197D0, 0x801197FC, 0x80119828, 0x80114F0C
SORT_FN = 0x800706B0
ENGINE = {0x8012297C: 'arg', 0x80032428: 'active', 0x80032170: 'task', 0x80030EA8: 'none',
          0x800648C8: 'none', 0x800067F8: 'task'}
BTP, MAKETHREAD, G3D, CAMERA = 0x801FCEB8, 0x803ADA48, 0x801CC1A5, 0x803ADAD0
HEAP, TABLE, SET_BASE, CB_BASE = 0x81000000, 0x81500000, 0x81580000, 0x81600000
WORK = {0x30: 0x34, 0x38: 0x3C, 0x40: 0x38, 0x10: 0x40}  # calloc total size -> header slot


def cb(k):
    return CB_BASE + 0x100 * k


# ---------------------------------------------------------------- scenarios
def scenarios():
    s = []
    s.append({'name': 'lists_and_exec', 'cbs': {}, 'ops': [
        {'op': 'create', 'im': 0, 'level': 0, 'exec': cb(1)},
        {'op': 'create', 'im': 2, 'level': 2, 'exec': cb(2)},
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(3)},
        {'op': 'create', 'im': 0, 'level': 5, 'exec': cb(4)},
        {'op': 'create', 'im': 0, 'level': 6, 'exec': cb(5)},
        {'op': 'create', 'im': 8, 'level': 1, 'exec': 0},
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(6)},
        {'op': 'exec'}, {'op': 'exec'}, {'op': 'clear'}, {'op': 'exec'},
    ]})
    s.append({'name': 'free_and_destroy', 'cbs': {cb(2): [['free_self']]}, 'ops': [
        {'op': 'create', 'im': 0xF, 'level': 3, 'exec': cb(1), 'slots': {'24': cb(20)},
         'set': 0, 'twp': {'44': 1}},
        {'op': 'create', 'im': 2, 'level': 3, 'exec': cb(2), 'slots': {'24': cb(21)}, 'set': 1},
        {'op': 'create', 'im': 0, 'level': 3, 'exec': cb(3)},
        {'op': 'child', 'im': 2, 'exec': cb(4), 'parent': 0, 'slots': {'24': cb(22)}},
        {'op': 'child', 'im': 0, 'exec': cb(5), 'parent': 0, 'slots': {'24': cb(23)}},
        {'op': 'child', 'im': 0, 'exec': cb(6), 'parent': 3},
        {'op': 'exec'}, {'op': 'exec'}, {'op': 'destroy', 'task': 2},
        {'op': 'free', 'task': 0}, {'op': 'exec'}, {'op': 'destroy_all'},
    ]})
    s.append({'name': 'head_destroys_itself', 'cbs': {cb(1): [['destroy_self']]}, 'ops': [
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(1)},
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(2)},
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(1)},
        {'op': 'create', 'im': 0, 'level': 2, 'exec': cb(3)},
        {'op': 'exec'}, {'op': 'exec'},
    ]})
    s.append({'name': 'create_during_exec', 'cbs': {
        cb(1): [['create', 0, 4, cb(2)], ['set_exec', cb(3)]],
        cb(2): [['child', 2, cb(4)], ['set_exec', cb(5)]],
        cb(4): [['set_exec', 0x80071F58]],
    }, 'ops': [
        {'op': 'create', 'im': 2, 'level': 4, 'exec': cb(1)},
        {'op': 'create', 'im': 0, 'level': 4, 'exec': cb(6)},
        {'op': 'exec'}, {'op': 'exec'}, {'op': 'exec'},
    ]})
    s.append({'name': 'free_task_c_children', 'cbs': {
        cb(1): [['set_exec', FREEC]],
        cb(2): [['free_self']],
        cb(3): [['set_exec', cb(2)]],
        cb(4): [['set_exec', cb(3)]],
    }, 'ops': [
        {'op': 'create', 'im': 2, 'level': 2, 'exec': cb(1), 'slots': {'24': cb(10)}},
        {'op': 'child', 'im': 0, 'exec': cb(2), 'parent': 0, 'slots': {'24': cb(11)}},
        {'op': 'child', 'im': 0, 'exec': cb(3), 'parent': 0, 'slots': {'24': cb(12)}},
        {'op': 'child', 'im': 0, 'exec': cb(4), 'parent': 0},
        {'op': 'child', 'im': 0, 'exec': cb(5), 'parent': 2},
        {'op': 'exec'}, {'op': 'exec'}, {'op': 'exec'}, {'op': 'exec'}, {'op': 'exec'},
        {'op': 'exec'},
    ]})
    f = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
    disp = {'20': cb(30), '28': cb(31), '32': cb(32), '36': cb(33), '40': cb(34), '44': cb(35)}
    ops = []
    for lvl, z in ((0, 5.0), (0, -2.0), (1, 1.0), (2, 3.0), (2, -7.0), (2, 0.5), (3, 9.0),
                   (5, -1.0), (6, 2.0)):
        ops.append({'op': 'create', 'im': 2, 'level': lvl, 'exec': 0, 'slots': disp,
                    'twp': {'20': f(0.0), '24': f(1.0), '28': f(z)}})
    ops.append({'op': 'create', 'im': 0, 'level': 2, 'exec': 0, 'slots': disp})
    ops.append({'op': 'child', 'im': 2, 'exec': 0, 'parent': 3,
                'slots': {'20': cb(40), '32': cb(41), '44': cb(42)}})
    ops.append({'op': 'child', 'im': 0, 'exec': 0, 'parent': 9, 'slots': {'32': cb(43)}})
    for mode, cam, active in ((0, 0, 0), (1, 0, 0), (1, 1, 0), (2, 1, 1), (2, 0, 1), (3, 0, 0)):
        ops.append({'op': 'display', 'mode': mode, 'cam': cam, 'active': active})
    ops.append({'op': 'shadow'})
    s.append({'name': 'display_pass', 'cbs': {}, 'ops': ops})
    s.append({'name': 'destroy_children_directly', 'cbs': {}, 'ops': [
        {'op': 'create', 'im': 2, 'level': 3, 'exec': cb(1),
         'twp': {'8': 0x11111111, '12': 0x22222222, '16': 0x33333333, '20': 0x44444444,
                 '24': 0x55555555, '28': 0x66666666, '32': 0x77777777}},
        {'op': 'child', 'im': 2, 'exec': cb(2), 'parent': 0, 'slots': {'24': cb(5)}},
        {'op': 'child', 'im': 0, 'exec': cb(3), 'parent': 0, 'slots': {'24': cb(6)}},
        {'op': 'child', 'im': 2, 'exec': cb(4), 'parent': 0, 'slots': {'24': cb(7)}},
        {'op': 'create', 'im': 0, 'level': 4, 'exec': cb(8)},
        {'op': 'create', 'im': 0, 'level': 0, 'exec': cb(9)},
        {'op': 'destroy', 'task': 2}, {'op': 'destroy', 'task': 1}, {'op': 'exec'},
        {'op': 'freec', 'task': 0}, {'op': 'destroy_stage'}, {'op': 'exec'},
    ]})
    return s


# ---------------------------------------------------------------- PPC stubs
def li(r, v):
    return 0x38000000 | r << 21 | (v & 0xFFFF)


def lis(r, v):
    return 0x3C000000 | r << 21 | (v & 0xFFFF)


def ori(ra, rs, v):
    return 0x60000000 | rs << 21 | ra << 16 | (v & 0xFFFF)


def load32(r, v):
    return [lis(r, v >> 16), ori(r, r, v & 0xFFFF)]


MR_R3_R31, MR_R5_R31 = 0x7FE3FB78, 0x7FE5FB78
PROLOGUE = [0x7C0802A6, 0x90010004, 0x9421FFE0, 0x93E1001C, 0x7C7F1B78]
EPILOGUE = [0x80010024, 0x83E1001C, 0x38210020, 0x7C0803A6, 0x4E800020]


def stub_code(base, actions):
    code = list(PROLOGUE)

    def bl(t):
        code.append(0x48000001 | ((t - (base + 4 * len(code))) & 0x3FFFFFC))
    for a in actions:
        k = a[0]
        if k in ('free_self', 'destroy_self', 'freec_self'):
            code.append(MR_R3_R31)
            bl({'free_self': FREE, 'destroy_self': DESTROY, 'freec_self': FREEC}[k])
        elif k == 'destroy':
            ad = TABLE + 4 * a[1]
            code += [lis(3, (ad + 0x8000) >> 16), 0x80630000 | (ad & 0xFFFF)]
            bl(DESTROY)
        elif k == 'create':
            code += [li(3, a[1]), li(4, a[2])] + load32(5, a[3]) + [li(6, 0)]
            bl(CET)
        elif k == 'child':
            code += [li(3, a[1])] + load32(4, a[2]) + [MR_R5_R31]
            bl(CCT)
        elif k == 'set_exec':
            code += load32(0, a[1]) + [0x901F0010]
        else:
            raise ValueError(k)
    return code + EPILOGUE


# ---------------------------------------------------------------- harness
class Oracle:
    def __init__(self, dol, sc):
        self.m = m = dol.machine(boot=False)
        self.heap = HEAP
        self.ids = {}       # task block address -> id
        self.work = {}      # work block address -> (id, header offset)
        self.scratch = set()
        self.freed = set()
        self.last_task = None
        self.events = []
        self.active = 0
        for p, fn in ((MALLOC, self.malloc), (CALLOC, self.calloc), (SYFREE, self.free),
                      (CALC_POINT, self.calc_point)):
            m.stub(p, fn)
        for addr, kind in ENGINE.items():
            m.stub(addr, self.engine(addr, kind))
        for k in range(64):
            a = cb(k)
            code = stub_code(a, sc['cbs'].get(str(a), sc['cbs'].get(a, [])))
            for i, w in enumerate(code):
                m.put(a + 4 * i, 'I', w)
            m.u.hook_add(UC_HOOK_CODE, self.log_cb, begin=a, end=a)
        for k in range(4):
            m.put(SET_BASE + 8 * k + 2, 'H', 3)
        m.call(CLEAR_BTP)

    def alloc(self, n):
        a = self.heap
        self.heap += (n + 0x1F) & ~0x1F
        self.m.u.mem_write(a, bytes(n))
        return a

    def malloc(self, m):
        n = m.gpr(3)
        a = self.alloc(n)
        if n == 0x50:
            self.ids[a] = len(self.ids)
            self.last_task = a
            m.put(TABLE + 4 * self.ids[a], 'I', a)
        m.gpr(3, a)

    def calloc(self, m):
        n = m.gpr(3) * m.gpr(4)
        a = self.alloc(n)
        lr = m.u.reg_read(__import__('unicorn').ppc_const.UC_PPC_REG_LR)
        if SORT_FN <= lr < SORT_FN + 0x228:
            self.scratch.add(a)
        else:
            self.work[a] = (self.ids[self.last_task], WORK[n])
        m.gpr(3, a)

    def free(self, m):
        a = m.gpr(3)
        assert a not in self.freed, hex(a)
        self.freed.add(a)
        if a in self.scratch:
            return
        if a in self.work:
            i, off = self.work[a]
            self.events.append(['free_work', i, off])
        else:
            self.events.append(['free_task', self.ids[a]])

    def calc_point(self, m):
        src, dst = m.gpr(4), m.gpr(5)
        m.u.mem_write(dst, bytes(m.u.mem_read(src, 12)))

    def engine(self, addr, kind):
        def f(m):
            if kind == 'arg':
                self.events.append(['engine', addr, [m.gpr(3)]])
            elif kind == 'task':
                args = [self.ids[m.gpr(3)]] + ([m.gpr(4), 0x803AF524] if addr == 0x80032170 else [])
                self.events.append(['engine', addr, args])
            elif kind == 'active':
                self.events.append(['engine', addr, [self.active]])
                m.gpr(3, self.active)
            else:
                self.events.append(['engine', addr, []])
        return f

    def log_cb(self, uc, pc, size, _):
        self.events.append(['call', pc, self.ids[self.m.gpr(3)]])

    def tid(self, p):
        return None if p == 0 else self.ids.get(p, -1)

    def state(self):
        m = self.m
        live = [a for a in self.ids if a not in self.freed]
        tasks = []
        for a in sorted(live, key=lambda x: self.ids[x]):
            w = [m.get(a + o, 'I') for o in range(0, 0x50, 4)]
            ocp = w[0x30 // 4]
            twp = w[0x34 // 4]
            tasks.append({
                'id': self.ids[a], 'next': self.tid(w[0]), 'prev': self.tid(w[1]),
                'parent': self.tid(w[2]), 'child': self.tid(w[3]),
                'slots': w[4:12],
                'set': None if ocp == 0 else (ocp - SET_BASE) // 8,
                'work': [int(w[o // 4] != 0) for o in (0x34, 0x38, 0x3C, 0x40)],
                'twp': None if twp == 0 else bytes(m.u.mem_read(twp, 0x30)).hex(),
            })
        return {
            'btp': [self.tid(m.get(BTP + 4 * i, 'I')) for i in range(8)],
            'makethread': m.get(MAKETHREAD, 'I'),
            'set_flags': [m.get(SET_BASE + 8 * k + 2, 'H') for k in range(4)],
            'tasks': tasks,
        }

    def task_addr(self, i):
        return next(a for a, k in self.ids.items() if k == i)

    def setup(self, op, a):
        m = self.m
        for off, v in op.get('slots', {}).items():
            m.put(a + int(off), 'I', v)
        if 'set' in op:
            m.put(a + 0x30, 'I', SET_BASE + 8 * op['set'])
        twp = m.get(a + 0x34, 'I')
        for off, v in op.get('twp', {}).items():
            m.put(twp + int(off), 'I', v)

    def run(self, op):
        m, k = self.m, op['op']
        if k == 'create':
            m.call(CET, r3=op['im'], r4=op['level'], r5=op['exec'], r6=0)
            self.setup(op, m.gpr(3))
        elif k == 'child':
            m.call(CCT, r3=op['im'], r4=op['exec'], r5=self.task_addr(op['parent']))
            self.setup(op, m.gpr(3))
        elif k in ('free', 'destroy', 'freec'):
            m.call({'free': FREE, 'destroy': DESTROY, 'freec': FREEC}[k], r3=self.task_addr(op['task']))
        elif k == 'exec':
            m.call(EXEC_PASS)
        elif k == 'display':
            m.put(G3D, 'B', op['mode'])
            m.put(CAMERA, 'i', op['cam'])
            self.active = op['active']
            m.call(DISPLAY_PASS)
        elif k == 'shadow':
            for level in range(6):
                m.call(SHADOW_LEVEL, r3=level)
        elif k == 'destroy_all':
            m.call(DESTROY_ALL)
        elif k == 'destroy_stage':
            m.call(DESTROY_STAGE)
        elif k == 'clear':
            m.call(CLEAR_BTP)
        else:
            raise ValueError(k)


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    out = []
    for sc in scenarios():
        o = Oracle(dol, sc)
        steps = []
        for op in sc['ops']:
            o.events = []
            o.run(op)
            steps.append({'events': o.events, 'state': o.state()})
        out.append({'name': sc['name'], 'cbs': {str(k): v for k, v in sc['cbs'].items()},
                    'ops': sc['ops'], 'steps': steps})
    json.dump({'scenarios': out}, sys.stdout)


if __name__ == '__main__':
    if not os.environ.get('SA2_MAIN_DOL'):
        print('skipped: SA2_MAIN_DOL is not set', file=sys.stderr)
        sys.exit(0)
    main()
