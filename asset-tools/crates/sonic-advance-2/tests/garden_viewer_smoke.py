"""Exercise the real desktop controls against the offline Rust executable.
Usage: python tests/garden_viewer_smoke.py /path/to/a2ne-garden ROM
"""
import hashlib
from pathlib import Path
import sys
import tkinter as tk
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import play_garden


def widgets(widget):
    for child in widget.winfo_children():
        yield child
        yield from widgets(child)


def smoke(game):
    window = tk.Tk()
    window.withdraw()
    errors = []
    checks = iter(("egg", "hatch", "game"))
    deadline = None
    phase = next(checks)
    egg_image = None
    chao_image = None

    def close():
        if deadline is not None:
            window.after_cancel(deadline)
        window.tk.call(window.protocol("WM_DELETE_WINDOW"))

    def timeout():
        errors.append(RuntimeError(f"{game} viewer timed out during {phase}"))
        close()

    def check():
        nonlocal phase, egg_image, chao_image
        try:
            controls = list(widgets(window))
            labels = [w for w in controls if isinstance(w, tk.Label)]
            canvas = next((w for w in labels if hasattr(w, "image")), None)
            ready = {"egg": " · Egg ", "hatch": " · Mood ", "game": game + " · "}
            if canvas is not None and any(ready[phase] in w["text"] for w in labels):
                image = canvas.image
                assert (image.width(), image.height()) == (480, 320)
                pixels = image.tk.call(str(image), "data", "-format", "ppm")
                digest = hashlib.sha256(pixels).digest()
                if phase == "game":
                    assert digest != chao_image
                    close()
                    return
                if phase == "egg":
                    egg_image, button = digest, "Hatch"
                else:
                    assert digest != egg_image, "hatching did not change the Garden image"
                    chao_image, button = digest, game
                next(w for w in controls if isinstance(w, tk.Button) and w["text"] == button).invoke()
                phase = next(checks)
            window.after(30, check)
        except Exception as error:
            errors.append(error)
            close()

    window.after(30, check)
    deadline = window.after(6000, timeout)
    with patch.object(play_garden.tk, "Tk", return_value=window):
        play_garden.main()
    if errors:
        raise errors[0]


if __name__ == "__main__":
    for name in ("Memory", "Bounce"):
        smoke(name)
    print("Real Tk controls: egg, hatched Chao, Memory and Bounce displayed distinct 480x320 frames.")
