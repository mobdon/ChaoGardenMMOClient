"""GSNE8P main.dol address of each Steam stage REL import addend (src/gsne8p_imports.rs)."""
import re
from pathlib import Path

IMPORTS = {int(a, 16): int(b, 16) for a, b in re.findall(
    r"\(0x([0-9a-f]{8}), 0x([0-9a-f]{8})\)",
    (Path(__file__).resolve().parent.parent / "src" / "gsne8p_imports.rs").read_text())}


def steam_rel(entry, module):
    """A Steam REL relocation entry (delta, kind, section, addend) with an
    engine import moved to its GSNE8P address."""
    delta, kind, section, addend = entry
    if module == 0 and kind in (1, 4, 6, 10):
        addend = IMPORTS[addend]
    return delta, kind, section, addend
