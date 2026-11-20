"""Oracle for sa2-runtime's stage load: main.dol's fn_8001EB80 with the GC
stg11D.rel loaded under Unicorn (sa2/tests/rel_load.py) so its subprogram init
fn_11_A434 runs natively, after the session init of session_ppc.py.

The REL loader itself (fn_8001A3BC) is stubbed: the oracle loads the module
first and sets ActiveSubprg as its prolog would. File loaders and the other
engine routines are stubbed and logged. Needs SA2_STG11D_REL (the GC disc's
stg11D.rel; never committed). Prints JSON.

Without SA2_MAIN_DOL or SA2_STG11D_REL it prints nothing and exits 0.
"""
import json, os, struct, sys
from pathlib import Path

if not os.environ.get('SA2_MAIN_DOL') or not os.environ.get('SA2_STG11D_REL'):
    print('skipped: SA2_MAIN_DOL or SA2_STG11D_REL is not set', file=sys.stderr)
    sys.exit(0)

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'sa2' / 'tests'))
import session_ppc  # noqa: E402
from session_ppc import SessionOracle, Dol, TABLES  # noqa: E402
from rel_load import load_rel  # noqa: E402
import load_ppc  # noqa: E402

STAGE_LOAD, REL_BASE, ACTIVE_SUBPRG = 0x8001EB80, 0x81200000, 0x803AD804
MAIN_STUBS = {0x8011C2C8: 1, 0x8011F524: 1, 0x8011F550: 1, 0x8011C2A4: 0, 0x80067FF4: 0,
              0x8001A3BC: 0, 0x8001EDFC: 0, 0x8001DDD8: 0, 0x80026490: 0, 0x80064258: 0,
              0x800324E4: 2, 0x800245EC: 4, 0x80072270: 2, 0x80072218: 2, 0x80033710: 0,
              0x800C3CD8: 0, 0x8011E154: 0, 0x8011E150: 0, 0x8002506C: 2, 0x80025028: 2,
              0x80025048: 1, 0x8002503C: 1, 0x80011A04: 1, 0x80011050: 1, 0x8006D500: 2}
REL_STUBS = {0xC2F0: 1, 0x152EC: 2, 0x330DC: 1, 0x28038: 1}
CLEAR = {'5870': (0x801E5870, 0x180), '5630': (0x801E5630, 0x240)}
SET_WORDS = {'ada08': 0x803ADA08, 'ad468': 0x803AD468, 'ada0c': 0x803ADA0C, 'ada18': 0x803ADA18,
             'ada1c': 0x803ADA1C, 'rows': 0x803ADA20, 'file': 0x803ADA24, 'file_arg': 0x803ADA28,
             'object_list': 0x803ADA2C}
ENTRIES, OBJTABLE, CALLBACKS, T77D8 = 0x801F5968, 0x801E5D28, 0x801A2A18, 0x801E77D8


class StageOracle(SessionOracle):
    def __init__(self, dol, sc):
        super().__init__(dol, sc)
        m = self.m
        rel = Path(os.environ['SA2_STG11D_REL']).read_bytes()
        self.sec = load_rel(m, rel, REL_BASE)
        self.layout = {'text': self.sec[1], 'rodata': self.sec[4], 'data': self.sec[5],
                       'bss': self.sec[6]}
        m.put(ACTIVE_SUBPRG, 'I', self.layout['data'] + 0x5D850)
        for a, n in MAIN_STUBS.items():
            m.stub(a, self.int_logger(a, n))
        m.stub(0x80065048, self.set_file)
        for off, n in REL_STUBS.items():
            a = self.layout['text'] + off
            m.stub(a, self.int_logger(a, n))
        for a, n in CLEAR.values():
            m.u.mem_write(a, b'\xff' * n)
        m.u.mem_write(ENTRIES, b'\xff' * (0x600 * 0x10))
        for a, n in ((OBJTABLE, 300 * 8), (CALLBACKS, 0x30), (T77D8, 0x38), (0x803ADC30, 4),
                     (0x803AD718, 4)):
            m.u.mem_write(a, bytes(n))
        self.objlist = []
        a = self.layout['data'] + 0x5B344
        while True:
            idx = m.get(a, 'h')
            self.objlist.append([idx, m.get(a + 2, 'h'), m.get(a + 4, 'I')])
            if idx == -1:
                break
            a += 8

    def set_file(self, m):
        self.events.append(['engine', 0x80065048, [m.gpr(3), m.gpr(4)]])
        m.gpr(3, 0)  # SET file loading is not ported: null, as for a missing file

    def free(self, m):
        a = m.gpr(3)
        owner = next(((i, off) for t, i in self.ids.items() for off in (0x38, 0x40)
                      if t not in self.freed and m.get(t + off, 'I') == a), None)
        if owner and any(x == a for x, n in self.others):
            assert a not in self.freed, hex(a)
            self.freed.add(a)
            self.events.append(['free_work', owner[0], owner[1]])
            return
        super().free(m)

    def tid(self, p):
        return None if p == 0 else self.ids.get(p, -1)

    def stage_state(self):
        m = self.m
        out = {'b81e': m.get(0x803AD81E, 'h'), 'b824': m.get(0x803AD824, 'h')}
        for k, (a, n) in CLEAR.items():
            out['cleared_' + k] = bytes(m.u.mem_read(a, n)) == bytes(n)
        s = {k: m.get(a, 'I') for k, a in SET_WORDS.items()}
        s['task'] = self.tid(m.get(0x803ADA10, 'I'))
        s['ada14'] = m.get(0x803ADA14, 'h')
        ok = True
        for i in range(0x5FF):
            e = ENTRIES + 0x10 * i
            ok &= m.get(e, 'H') == 0 and m.get(e + 2, 'H') == 0 and m.get(e + 8, 'I') == 0 \
                and m.get(e + 0xC, 'I') == 0
        s['entries_cleared'] = ok
        out['set'] = s
        out['object_table'] = [[m.get(OBJTABLE + 8 * i + 2, 'h'), m.get(OBJTABLE + 8 * i + 4, 'I')]
                               for i in range(300)]
        out['callbacks'] = [m.get(CALLBACKS + 4 * i, 'I') for i in range(12)]
        out['table_77d8'] = [m.get(T77D8 + 4 * i, 'I') for i in range(14)]
        out['adc30'] = m.get(0x803ADC30, 'I')
        out['ad718'] = m.get(0x803AD718, 'I')
        out['death_task'] = self.tid(m.get(0x803AD9D0, 'I'))
        out['death_data'] = m.get(0x803AD9D4, 'I')
        b, d = self.layout['bss'], self.layout['data']
        out['bss20'] = self.tid(m.get(b + 0x20, 'I'))
        out['bss24'] = m.get(b + 0x24, 'I')
        out['bss4d0'] = self.tid(m.get(b + 0x4D0, 'I'))
        out['slots_5e534'] = [self.tid(m.get(d + 0x5E534 + 4 * i, 'I')) for i in range(2)]
        out['chars'] = [m.get(0x803AD828, 'i'), m.get(0x803AD82C, 'i')]
        return out

    def run(self, op):
        if op['op'] == 'state':
            for i, (v, p) in op.get('objtable', {}).items():
                self.m.put(OBJTABLE + 8 * int(i) + 2, 'h', v)
                self.m.put(OBJTABLE + 8 * int(i) + 4, 'I', p)
        if op['op'] == 'stage':
            self.m.call(STAGE_LOAD, count=50_000_000)
            return
        super().run(op)


def scenarios():
    up = [0] * 0x1D
    return [
        {'name': 'iron_gate_load', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up, 'w420': -1},
            {'op': 'session'}, {'op': 'stage'}]},
        {'name': 'eggman_swap', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up, 'w420': -1},
            {'op': 'session'}, {'op': 'state', 'chars': [3, 0]}, {'op': 'stage'}]},
        {'name': 'object_entry_kept', 'cbs': {}, 'ops': [
            {'op': 'state', 'stage': 11, 'chars': [7, 0], 'upgrades': up, 'w420': -1,
             'objtable': {'86': [5, 0x81111110]}},
            {'op': 'session'}, {'op': 'stage'}]},
    ]


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    out = []
    layout = objlist = None
    for sc in scenarios():
        o = StageOracle(dol, sc)
        layout, objlist = o.layout, o.objlist
        steps = []
        for op in sc['ops']:
            o.events, o.ret = [], None
            o.run(op)
            steps.append({'events': o.events, 'state': o.state(), 'load': o.load_state(),
                          'session': o.session_state(), 'stage': o.stage_state()})
        out.append({'name': sc['name'], 'cbs': {}, 'ops': sc['ops'], 'steps': steps})
    json.dump({'tables': TABLES, 'tex_size': load_ppc.TEX_SIZE, 'coll': load_ppc.COLL,
               'layout': layout, 'object_list': objlist, 'scenarios': out}, sys.stdout)


if __name__ == '__main__':
    main()
