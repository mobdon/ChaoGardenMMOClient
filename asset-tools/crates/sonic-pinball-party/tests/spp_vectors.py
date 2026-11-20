"""Where the Pinball Party test vectors live.

The `*-vectors.json` files are generated from the user's own ROM by the
`*_reference.py` scripts, so they are not committed. They live in the local,
git-ignored `.local/research/sonic-pinball-party-vectors/` at the client
checkout root, or in `SPP_VECTORS_DIR`. The Rust crate's build.rs reads the
same location.
"""
import os, subprocess, sys
from pathlib import Path

REL = Path(".local/research/sonic-pinball-party-vectors")


def directory():
    if os.environ.get("SPP_VECTORS_DIR"):
        return Path(os.environ["SPP_VECTORS_DIR"])
    here = Path(__file__).resolve().parent
    for d in (here, *here.parents):
        if (d / REL).is_dir():
            return d / REL
    return here.parents[3] / REL  # client checkout root


def path(name):
    d = directory()
    d.mkdir(parents=True, exist_ok=True)
    return d / name


def extract_from_git(rev):
    """Write every vectors file committed at `rev` into the local directory."""
    prefix = "asset-tools/crates/sonic-pinball-party/tests/"
    names = subprocess.run(
        ["git", "ls-tree", "--name-only", rev, prefix],
        check=True, capture_output=True, text=True,
    ).stdout.split()
    names = [n for n in names if n.endswith("-vectors.json")]
    for n in names:
        data = subprocess.run(["git", "show", f"{rev}:{n}"], check=True, capture_output=True).stdout
        path(Path(n).name).write_bytes(data)
    return len(names)


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--extract-from-git":
        print(f"wrote {extract_from_git(sys.argv[2])} files to {directory()}")
    else:
        print(__doc__)
