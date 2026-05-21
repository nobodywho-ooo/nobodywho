#!/usr/bin/env python3
"""Inject the `__wasm_bindgen_emscripten_marker` custom section into a wasm.

wasm-bindgen-cli switches to Emscripten output mode (emits library_bindgen.js
in the Emscripten `addToLibrary({ ... })` format) when it finds a wasm
custom section with this exact name. The wasm-bindgen runtime tries to
emit one via `#[link_section = "__wasm_bindgen_emscripten_marker"]` on a
static, but the Rust → LLVM → wasm-ld chain for the
`wasm32-unknown-emscripten` target doesn't preserve plain `#[link_section]`
data as wasm custom sections — they end up merged into regular data
sections instead. (Same code is verified to work on `wasm32-unknown-unknown`
where LLVM's wasm backend does treat `#[link_section]` as custom-section
metadata.)

The marker payload is a single byte `0x01`. The wasm-bindgen-cli only
checks for the section's existence (via `module.customs.remove_raw(...)`)
— the byte value is not significant. We just need any non-empty payload
so walrus parses it as a valid custom section.

Usage:
  inject-emscripten-marker.py <input.wasm> <output.wasm>
"""
import sys

if len(sys.argv) != 3:
    print(__doc__, file=sys.stderr)
    sys.exit(2)

src_path, dst_path = sys.argv[1], sys.argv[2]

with open(src_path, "rb") as f:
    wasm = f.read()

# Wasm preamble: 4-byte magic ("\0asm") + 4-byte version (1).
if wasm[:4] != b"\x00asm" or len(wasm) < 8:
    print(f"error: {src_path} is not a wasm file (bad magic)", file=sys.stderr)
    sys.exit(1)

name = b"__wasm_bindgen_emscripten_marker"
data = b"\x01"


def leb128(n: int) -> bytes:
    """Encode an unsigned int as wasm/LLVM LEB128.

    For our values (small), this collapses to a single byte; the loop is
    here so the script doesn't quietly miscompose a section if someone
    later bumps the payload past 127 bytes.
    """
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


# Custom-section layout (wasm spec §5.5.1):
#   section_id (0x00) | leb128(payload_len) | leb128(name_len) | name | data
payload = leb128(len(name)) + name + data
section = b"\x00" + leb128(len(payload)) + payload

# Sections may appear in any order after the preamble (custom sections
# specifically are allowed anywhere). Insert ours right after the preamble
# so walrus' fast-path section iteration finds it.
new_wasm = wasm[:8] + section + wasm[8:]

with open(dst_path, "wb") as f:
    f.write(new_wasm)

print(
    f"ok: inserted {len(section)}-byte custom section "
    f"'{name.decode()}' ({len(wasm)} → {len(new_wasm)} bytes)"
)
