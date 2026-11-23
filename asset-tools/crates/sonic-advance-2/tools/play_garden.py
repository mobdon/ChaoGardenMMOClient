"""Offline A2NE Tiny Garden viewer. Native game state and pixels come from a2ne-garden.
Usage: python tools/play_garden.py PATH_TO_A2NE_GARDEN_EXE ROM [FLASH.sav|-] [SEED]
No network, game downloads, save overwrite, or third-party Python dependency.
"""
import json
import subprocess
import sys
import time
import tkinter as tk
from tkinter import filedialog, simpledialog
from pathlib import Path

HELP = "Arrows: D-pad    Z: A    X: B    Enter: Bounce pause / confirm    Click garden: place held item"
KEYS = {"z":1, "x":2, "Return":8, "Right":16, "Left":32, "Up":64, "Down":128}

class Cartridge:
    def __init__(self, args):
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        self.state = self.receive()

    def receive(self):
        line = self.proc.stdout.readline(65536)
        if not line:
            raise RuntimeError("Rust cartridge runner exited.")
        v = json.loads(line)
        if "error" in v:
            raise ValueError(v["error"])
        return v

    def send(self, line):
        self.proc.stdin.write((line+"\n").encode("utf-8"))
        self.proc.stdin.flush()

    def action(self, line):
        self.send(line)
        v = self.receive()
        if "scene" in v: self.state = v
        return v

    def picture(self):
        self.send("frame")
        header = self.proc.stdout.readline(65536)
        if header != b"P6\n":
            raise ValueError(json.loads(header).get("error","Invalid frame"))
        dims = self.proc.stdout.readline(32)
        scale = self.proc.stdout.readline(16)
        if dims != b"240 160\n" or scale != b"255\n":
            raise ValueError("Invalid cartridge frame dimensions")
        rgb = self.proc.stdout.read(240*160*3)
        if len(rgb) != 240*160*3:
            raise ValueError("Truncated cartridge frame")
        return header+dims+scale+rgb

    def close(self):
        try:
            self.send("quit")
            self.proc.wait(timeout=2)
        except (OSError, subprocess.TimeoutExpired):
            self.proc.kill()
            self.proc.wait()

def main():
    if len(sys.argv) not in (3,4,5):
        raise SystemExit(__doc__)
    root = tk.Tk()
    root.title("Tiny Chao Garden · A2NE cartridge practice")
    cart = Cartridge(sys.argv[1:])
    held = set()
    note, stats = tk.StringVar(),tk.StringVar()
    canvas = tk.Label(root, borderwidth=0)
    canvas.pack()
    tk.Label(root,textvariable=stats,wraplength=480).pack()
    tk.Label(root,text=HELP).pack()
    actions = tk.Frame(root); actions.pack()
    tk.Label(root,textvariable=note,wraplength=470).pack()
    counters = {"live":True, "stamp":time.monotonic(), "debt":0.0}

    def command(line):
        try:
            value = cart.action(line)
            if line=="shop":
                note.set("Fruit prices: "+str(value["fruitPrices"])+"  ·  Egg: "+str(value["egg"])+"  ·  Toy: "+str(value["toy"]))
            else: note.set("")
        except ValueError as e: note.set(str(e))

    def slot(title, keyword, upper):
        n=simpledialog.askinteger(title, "Slot or ID (0–%d)"%upper,parent=root,minvalue=0,maxvalue=upper)
        if n is not None: command(f"{keyword} {n}")

    def place(event):
        if cart.state.get("scene")=="garden": command(f"place {event.x//2} {event.y//2}")
    canvas.bind("<Button-1>",place)

    def save():
        p=filedialog.asksaveasfilename(title="New flash copy under .local", defaultextension=".sav",filetypes=[("Cartridge flash", "*.sav")])
        if p: command("save "+str(Path(p)))

    for k,(label,fn) in enumerate([("Memory",lambda:command("memory")),("Bounce",lambda:command("bounce")),
        ("Hatch",lambda:command("hatch")),("Shop",lambda:command("shop")),
        ("Buy fruit",lambda:slot("Buy fruit","buy fruit",6)),("Feed",lambda:slot("Feed from slot","feed",7)),
        ("Pull weed",lambda:slot("Pull weed","pull",15)),("Save copy",save),("Buy egg",lambda:slot("Buy offered egg","buy egg",11)),("Buy toy",lambda:slot("Buy offered toy","buy toy",2))]):
        tk.Button(actions,text=label,command=fn,takefocus=False).grid(row=k//4,column=k%4,sticky="ew")
    root.bind("<KeyPress>",lambda e:held.add(e.keysym) if e.keysym in KEYS else None)
    root.bind("<KeyRelease>",lambda e:held.discard(e.keysym))
    root.bind("<FocusOut>",lambda _:held.clear())

    def refresh():
        if not counters["live"]: return
        now=time.monotonic()
        counters["debt"] = min(4,counters["debt"] + (now-counters["stamp"])*59.7275)
        counters["stamp"] = now
        n=int(counters["debt"])
        if n:
            counters["debt"]-=n
            try:
                home=cart.state["scene"]=="garden"
                mask=0 if home else sum(KEYS[k] for k in held)
                cart.action(f"pad {mask} {n}")
                screen=tk.PhotoImage(data=cart.picture(),format="PPM").zoom(2)
                canvas.configure(image=screen); canvas.image=screen
                v=cart.state; s=v.get("state",v)
                if v["scene"]=="garden":
                    growth=v.get("growth")
                    resident=f"Mood {growth['mood']} · Belly {growth['belly']}" if growth else f"Egg {v['egg']}" if v['egg'] is not None else "No resident"
                    fruit=", ".join(f"{i}:{kind}" for i,kind in enumerate(v["fruitSlots"]) if kind is not None) or "none"
                    weeds=", ".join(str(row[0]) for row in v["weedSlots"] if row[3]) or "none"
                    stats.set(f"Garden · {v['wallet']} rings · {resident}\nFruit slots: {fruit} · Weeds ready to pull: {weeds}")
                    if not home and v.get("lastResult"):
                        name,cleared,earned=v["lastResult"]
                        note.set(f"{name.title()} {'cleared' if cleared else 'finished'} · {earned} rings earned")
                else:
                    stats.set(f"{v['scene'].title()} · {s.get('phase','Garden')} · {s.get('rings',v.get('wallet',0))} rings")
            except ValueError as e:
                note.set(str(e))
            except (OSError,RuntimeError) as e:
                note.set(str(e)); return
        root.after(8,refresh)

    def close():
        counters["live"]=False;root.destroy()
    root.protocol("WM_DELETE_WINDOW",close)
    try:
        refresh()
        root.mainloop()
    finally:
        cart.close()

if __name__=="__main__":main()
