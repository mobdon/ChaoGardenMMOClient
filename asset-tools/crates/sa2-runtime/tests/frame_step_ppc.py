"""Oracle for sa2-runtime's frame step: main.dol's in-stage frame
(fn_8001E45C, with fn_8001E89C), pause frame (fn_8001E12C), frame counter
(fn_80062B38) and display frames (fn_80070C20 / fn_80070BA8) under Unicorn.

The task routines and the display pass run natively (as in
task_system_ppc.py); every other engine routine the frame calls is stubbed
and logged with its integer arguments. Game-state memory (G, the frame's
sdata words, pads, player pointers) is set from each scenario. Prints JSON.

Without SA2_MAIN_DOL it prints nothing and exits 0.
"""
import json, os, struct, sys
from pathlib import Path

if __name__ == '__main__' and not os.environ.get('SA2_MAIN_DOL'):
    print('skipped: SA2_MAIN_DOL is not set', file=sys.stderr)
    sys.exit(0)

sys.path.insert(0, str(Path(__file__).resolve().parent))
from task_system_ppc import Oracle, Dol, cb  # noqa: E402

STAGE_FRAME, PAUSE_FRAME = 0x8001E45C, 0x8001E12C
G = 0x801CC168
SD = {  # name: (address, struct format)
    'ticks': (0x803AD8AC, 'i'), 'tick_index': (0x803AD8A4, 'i'), 'slow': (0x803AD856, 'B'),
    'mode': (0x803AD81A, 'h'), 'exit_request': (0x803AD81C, 'h'), 'stage': (0x803AD820, 'h'),
    'overlay': (0x803AD854, 'h'), 'pause_flag': (0x803AD839, 'b'), 'pause_cursor': (0x803AD837, 'B'),
    'pause_menu': (0x803AD838, 'B'), 'pause_timer': (0x803AD84C, 'h'), 'viewports': (0x803ADAD4, 'i'),
    'camera': (0x803ADAD0, 'i'), 'gx_ae139': (0x803AE139, 'B'),
}
PAUSE_ITEMS = (0x803AD840, 0x803AD844, 0x803AD848)
PADS, PADCOPY, W53A8, W5398 = 0x801E55D8, 0x801E5438, 0x801E53A8, 0x801E5398
PWP, PTP, CLOCKP = 0x801E7728, 0x801E7788, 0x803AD860
PAD_BASE, PWP_BASE, CLOCK = 0x81590000, 0x81590100, 0x81590200
RING, FRAME_END = 0x80062C8C, 0x80035B94
STUBS = {  # address: number of integer arguments logged
    0x800226E0: 0, 0x800648C4: 0, 0x80006B00: 0, 0x800D0904: 0, 0x80022EE0: 1, 0x80035B68: 0,
    0x80014CD8: 0, 0x801160E4: 0, 0x80061CBC: 0, 0x8001C810: 1, 0x80019CA0: 0, 0x80057524: 0,
    0x8011C2BC: 0, 0x8011C2B0: 0, 0x8011C310: 0, 0x80117C80: 1, 0x801179F8: 2, 0x80116E18: 1,
    0x80116E34: 1, 0x8011E5E0: 3, 0x8011C508: 4, 0x8006B978: 4, 0x8001B60C: 1, 0x8001B404: 3,
    0x80117D38: 0, 0x80117D74: 0, 0x80117D14: 0, 0x80070F5C: 0, 0x801014F0: 0,
}


class FrameOracle(Oracle):
    def __init__(self, dol, sc):
        super().__init__(dol, sc)
        m = self.m
        self.host = {'rings': 0, 'pressed': [0, 0], 'held': [0, 0], 'copy': 0, 'w53a8': 0,
                     'w5398': [0, 0], 'flags': [None, None], 'end': 0.0}
        for a, n in STUBS.items():
            m.stub(a, self.logger(a, n))
        m.stub(RING, self.ring)
        m.stub(FRAME_END, self.frame_end)
        m.put(CLOCKP, 'I', CLOCK)
        m.u.mem_write(G, bytes(0xF0))  # synthetic start: G zeroed (its .data values are not used)
        for i in range(2):
            m.put(PADS + 4 * i, 'I', PAD_BASE + 0x40 * i)
        self.apply_host()

    def logger(self, addr, n):
        def f(m):
            self.events.append(['engine', addr, [m.gpr(3 + k) for k in range(n)]])
        return f

    def ring(self, m):
        self.events.append(['engine', RING, [m.gpr(3)]])
        m.gpr(3, self.host['rings'] & 0xFFFF)

    def frame_end(self, m):
        self.events.append(['engine', FRAME_END, []])
        m.fpr(1, self.host['end'])

    def apply_host(self):
        m, h = self.m, self.host
        for i in range(2):
            m.put(PAD_BASE + 0x40 * i + 0x10, 'I', h['pressed'][i])
            m.put(PAD_BASE + 0x40 * i + 0x8, 'I', h['held'][i])
            m.put(W5398 + 4 * i, 'I', h['w5398'][i])
            if h['flags'][i] is None:
                m.put(PWP + 4 * i, 'I', 0)
            else:
                m.put(PWP + 4 * i, 'I', PWP_BASE + 0x40 * i)
                m.put(PWP_BASE + 0x40 * i + 0x10, 'h', h['flags'][i])
        m.put(PADCOPY + 0x10, 'I', h['copy'])
        m.put(W53A8, 'I', h['w53a8'])

    def frame_state(self):
        m = self.m
        out = {k: m.get(a, f) for k, (a, f) in SD.items()}
        out['g'] = bytes(m.u.mem_read(G, 0xF0)).hex()
        out['clock'] = list(m.u.mem_read(CLOCK, 3))
        out['pause_items'] = [m.get(a, 'I') for a in PAUSE_ITEMS]
        return out

    def run(self, op):
        m, k = self.m, op['op']
        if k == 'state':
            for off, v in op.get('g', {}).items():
                m.put(G + int(off), 'B', v)
            for off, v in op.get('g32', {}).items():
                m.put(G + int(off), 'I', v)
            for name, v in op.get('sd', {}).items():
                a, f = SD[name]
                m.put(a, f, v)
            for i, v in enumerate(op.get('pause_items', [])):
                m.put(PAUSE_ITEMS[i], 'I', v)
            for i, t in op.get('player_tasks', {}).items():
                m.put(PTP + 4 * int(i), 'I', 0 if t is None else self.task_addr(t))
        elif k == 'host':
            self.host.update({x: v for x, v in op.items() if x != 'op'})
            self.apply_host()
        elif k == 'frame':
            m.call(STAGE_FRAME, count=20_000_000)
            self.ret = m.gpr(3)
        elif k == 'pause':
            m.call(PAUSE_FRAME, count=20_000_000)
            self.ret = m.gpr(3)
        else:
            super().run(op)


def scenarios():
    f = lambda x: struct.unpack('>I', struct.pack('>f', x))[0]
    tasks = [
        {'op': 'create', 'im': 2, 'level': 0, 'exec': cb(1), 'slots': {'20': cb(10)}},
        {'op': 'create', 'im': 2, 'level': 2, 'exec': cb(2), 'slots': {'20': cb(11), '32': cb(12)},
         'twp': {'28': f(3.0)}},
        {'op': 'create', 'im': 2, 'level': 4, 'exec': cb(3), 'slots': {'28': cb(13)}},
    ]
    s = []
    s.append({'name': 'play_frames', 'cbs': {}, 'ops': tasks + [
        {'op': 'state', 'sd': {'mode': 0x10, 'stage': 11}, 'g': {'35': 1, '15': 2},
         'g32': {'124': 100}},
        {'op': 'host', 'rings': 120, 'end': 0.5},
        {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'},
        {'op': 'frame'},
        {'op': 'state', 'g': {'27': 99, '28': 59, '29': 58, '51': 2, '52': 1}},
        {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'},
        {'op': 'host', 'pressed': [8, 0]}, {'op': 'frame'},
        {'op': 'state', 'g': {'22': 1}}, {'op': 'frame'},
        {'op': 'state', 'g': {'22': 0, '55': 1}}, {'op': 'frame'},
        {'op': 'state', 'g': {'55': 0, '26': 1}}, {'op': 'frame'},
        {'op': 'state', 'sd': {'stage': 0x41}, 'g': {'26': 0}}, {'op': 'frame'},
        {'op': 'state', 'g': {'57': 1, '27': 0, '28': 0, '29': 0}}, {'op': 'frame'}, {'op': 'frame'},
        {'op': 'state', 'g': {'57': 0}}, {'op': 'frame'},
    ]})
    s.append({'name': 'two_player', 'cbs': {}, 'ops': tasks + [
        {'op': 'state', 'sd': {'mode': 0x10, 'viewports': 2, 'overlay': 2},
         'g': {'30': 1, '32': 1, '35': 1}},
        {'op': 'host', 'pressed': [0, 8]}, {'op': 'frame'},
        {'op': 'state', 'g': {'61': 1}, 'sd': {'camera': 0}}, {'op': 'frame'},
        {'op': 'state', 'g': {'34': 1, '61': 2}, 'sd': {'camera': 1}}, {'op': 'frame'},
        {'op': 'state', 'g': {'30': 0}}, {'op': 'frame'},
    ]})
    s.append({'name': 'demo_exit', 'cbs': {}, 'ops': tasks + [
        {'op': 'state', 'sd': {'mode': 0x10, 'gx_ae139': 7}, 'g': {'0': 1, '11': 1, '89': 5},
         'g32': {'120': 0x40}},
        {'op': 'host', 'flags': [0, 0x4000]}, {'op': 'frame'},
        {'op': 'state', 'sd': {'exit_request': 0}},
        {'op': 'host', 'flags': [None, None], 'copy': 8}, {'op': 'frame'},
        {'op': 'host', 'copy': 0, 'pressed': [8, 0]}, {'op': 'frame'},
        {'op': 'state', 'sd': {'mode': 7}}, {'op': 'frame'},
    ]})
    s.append({'name': 'solo_player', 'cbs': {}, 'ops': tasks + [
        {'op': 'create', 'im': 0, 'level': 1, 'exec': cb(4)},
        {'op': 'state', 'sd': {'mode': 0x10}, 'g': {'73': 1, '15': 2}, 'player_tasks': {'0': 3}},
        {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'}, {'op': 'frame'},
        {'op': 'state', 'g': {'0': 1, '32': 1, '11': 1}, 'g32': {'120': 0x40}},
        {'op': 'host', 'flags': [0x4000, None]}, {'op': 'frame'},
        {'op': 'state', 'g': {'73': 2}, 'player_tasks': {'1': 1}}, {'op': 'frame'},
    ]})
    s.append({'name': 'pause_menu', 'cbs': {}, 'ops': tasks + [
        {'op': 'state', 'sd': {'mode': 0x11, 'pause_menu': 2}, 'g': {'23': 1},
         'pause_items': [0x11, 0x22, 0x33]},
        {'op': 'pause'},
        {'op': 'host', 'w53a8': 0x10, 'w5398': [0x20, 0]}, {'op': 'pause'},
        {'op': 'state', 'sd': {'pause_timer': 0}}, {'op': 'pause'}, {'op': 'pause'},
        {'op': 'host', 'w5398': [0x10, 0]}, {'op': 'state', 'sd': {'pause_timer': 0}}, {'op': 'pause'},
        {'op': 'state', 'sd': {'pause_timer': 0x7FF0}}, {'op': 'pause'},
        {'op': 'state', 'sd': {'pause_timer': 0xF0}}, {'op': 'pause'},
        {'op': 'state', 'sd': {'pause_menu': 3, 'pause_cursor': 0}}, {'op': 'pause'},
        {'op': 'host', 'held': [0x600, 0]}, {'op': 'pause'},
        {'op': 'state', 'g': {'23': 2, '30': 1, '32': 1}, 'sd': {'viewports': 2}},
        {'op': 'host', 'held': [0, 0], 'w5398': [0, 0x20]}, {'op': 'pause'},
        {'op': 'state', 'sd': {'pause_cursor': 0xFF, 'pause_timer': 0}}, {'op': 'pause'},
    ]})
    return s


def main():
    dol = Dol(os.environ['SA2_MAIN_DOL'])
    out = []
    for sc in scenarios():
        o = FrameOracle(dol, sc)
        steps = []
        for op in sc['ops']:
            o.events, o.ret = [], None
            o.run(op)
            steps.append({'events': o.events, 'state': o.state(), 'frame': o.frame_state(),
                          'ret': o.ret})
        out.append({'name': sc['name'], 'cbs': {str(k): v for k, v in sc['cbs'].items()},
                    'ops': sc['ops'], 'steps': steps})
    json.dump({'scenarios': out}, sys.stdout)


if __name__ == '__main__':
    main()
