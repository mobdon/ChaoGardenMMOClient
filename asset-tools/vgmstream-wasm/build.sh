#!/usr/bin/env bash
# Builds vgmstream's core (no external codec libraries) plus shim.c into
# vgmstream.wasm (wasm32-wasi reactor) with zig's bundled clang.
#   ZIG=<path to zig>  VGMSTREAM_SRC=<vgmstream checkout at the pinned tag>  ./build.sh [out dir]
# Pinned: vgmstream r2168 (d9875b1fff307e4759dfb85e49e3d5e6fb196f7c).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ZIG="${ZIG:-zig}"
SRC="${VGMSTREAM_SRC:?set VGMSTREAM_SRC to a vgmstream checkout (git clone --depth 1 --branch r2168 https://github.com/vgmstream/vgmstream)}"
OUT="${1:-$here/web}"
want=d9875b1fff307e4759dfb85e49e3d5e6fb196f7c
got="$(git -C "$SRC" rev-parse HEAD 2>/dev/null || echo unknown)"
[ "$got" = "$want" ] || echo "warning: vgmstream at $got, pinned $want" >&2

obj="$(mktemp -d)"
trap 'rm -rf "$obj"' EXIT
flags=(-target wasm32-wasi -O2 -DNDEBUG -DVGM_LOG_OUTPUT=0 -I"$SRC/src" -Wno-everything)
# Compile in parallel; each file gets its own object.
find "$SRC/src" -name '*.c' > "$obj/list"
echo "$here/shim.c" >> "$obj/list"
n=0
while read -r c; do
  n=$((n + 1))
  "$ZIG" cc "${flags[@]}" -c "$c" -o "$obj/$n.o" &
  if (( n % 16 == 0 )); then wait; fi
done < "$obj/list"
wait
mkdir -p "$OUT"
# Windows limits argv length: pass the objects in a response file.
for o in "$obj"/*.o; do if command -v cygpath >/dev/null; then cygpath -m "$o"; else echo "$o"; fi; done > "$obj/objs.rsp"
rsp="$obj/objs.rsp"; command -v cygpath >/dev/null && rsp="$(cygpath -m "$rsp")"
"$ZIG" cc "${flags[@]}" -mexec-model=reactor -Wl,--strip-all -o "$OUT/vgmstream.wasm" "@$rsp"
ls -l "$OUT/vgmstream.wasm"
