#!/usr/bin/env bash
# Inject the rust-lang/libc#5156 pthread-size fix into the active nightly's
# rust-src, so `-Zbuild-std` compiles a std whose `pthread_attr_t` is correctly
# sized on wasm64 (MEMORY64) and `std::thread::spawn` works.
#
# WHY (and why here): `-Zbuild-std` recompiles `std` from rust-src and resolves
# the SYSROOT's libc separately from the app workspace, so the workspace
# `[patch]` never reaches it. The crash — std's `Thread::new` overruns
# `pthread_attr_t` (44 vs 88 bytes on wasm64) — is std-side, so the fix must be
# injected into rust-src's `library/Cargo.toml`. `build-pkg-emscripten-wasm64.sh`
# refuses to build without it.
#
# Self-contained: reads the EXACT libc version std locks, downloads that source,
# applies the 3-line #5156 fix, and patches rust-src to it. Idempotent — safe to
# re-run; restore `library/Cargo.toml.wasm64-orig` to undo. #5156 is merged
# upstream (2026-06-14) but not yet in a released libc, so this is still
# required; delete the whole mechanism once a nightly's std bumps to a libc
# that already includes #5156 (the fix step then becomes a no-op).
#
# Override the patched-clone location with LIBC_PATCH_DIR (default /tmp/libc-wasm64).
set -euo pipefail

RUST_SRC="$(rustc +nightly --print sysroot)/lib/rustlib/src/rust"
LIB="$RUST_SRC/library"
LOCK="$LIB/Cargo.lock"
MANIFEST="$LIB/Cargo.toml"
CLONE="${LIBC_PATCH_DIR:-/tmp/libc-wasm64}"
EM_REL="src/unix/linux_like/emscripten/mod.rs"

if [[ ! -f "$LOCK" ]]; then
  echo "error: rust-src not found ($LOCK)." >&2
  echo "       Add it: rustup component add rust-src --toolchain nightly" >&2
  exit 1
fi

# 1. The exact libc version std locks (e.g. 0.2.185) — robust to nightly bumps.
LIBC_VER="$(awk '/^name = "libc"$/{f=1;next} f&&/^version = /{gsub(/[" ]/,"",$3);print $3;exit}' "$LOCK")"
[[ -n "$LIBC_VER" ]] || { echo "error: could not read libc version from $LOCK" >&2; exit 1; }
echo "==> std locks libc $LIBC_VER"

# 2. Fetch that exact source into $CLONE (cached by version marker).
if [[ ! -f "$CLONE/$EM_REL" || "$(cat "$CLONE/.libc-ver" 2>/dev/null || true)" != "$LIBC_VER" ]]; then
  echo "==> downloading libc $LIBC_VER source"
  rm -rf "$CLONE"; mkdir -p "$CLONE"
  curl -fsSL "https://static.crates.io/crates/libc/libc-$LIBC_VER.crate" \
    | tar -xz -C "$CLONE" --strip-components=1
  echo "$LIBC_VER" > "$CLONE/.libc-ver"
fi

# 3. Apply the #5156 pthread-size fix (idempotent; no-op if std libc already has it).
python3 - "$CLONE/$EM_REL" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read(); orig = s
s = s.replace(
    "        __size: [u32; 11],",
    "        // libc#5156: 44 bytes on wasm32, 88 on wasm64 (MEMORY64).\n        __size: [usize; 11],")
s = s.replace(
    "pub const __SIZEOF_PTHREAD_RWLOCK_T: usize = 32;",
    '#[cfg(target_pointer_width = "32")]\npub const __SIZEOF_PTHREAD_RWLOCK_T: usize = 32;\n'
    '#[cfg(target_pointer_width = "64")]\npub const __SIZEOF_PTHREAD_RWLOCK_T: usize = 56;')
s = s.replace(
    "pub const __SIZEOF_PTHREAD_MUTEX_T: usize = 24;",
    '#[cfg(target_pointer_width = "32")]\npub const __SIZEOF_PTHREAD_MUTEX_T: usize = 24;\n'
    '#[cfg(target_pointer_width = "64")]\npub const __SIZEOF_PTHREAD_MUTEX_T: usize = 40;')
if "__size: [usize; 11]" not in s:
    sys.stderr.write("error: could not apply libc#5156 fix — pthread_attr_t form changed?\n")
    sys.exit(1)
open(p, "w").write(s)
print("   fix applied" if s != orig else "   fix already present (std libc may already include #5156)")
PY

# 4. Point rust-src's [patch.crates-io] at the patched clone (idempotent).
#    Insert UNDER the existing [patch.crates-io] table (rust-src already has one)
#    rather than appending a duplicate header (which TOML rejects).
python3 - "$MANIFEST" "$CLONE" <<'PY'
import sys
manifest, clone = sys.argv[1], sys.argv[2]
t = open(manifest).read()
if "libc = { path" in t:
    print("   rust-src already patched"); sys.exit(0)
line = 'libc = { path = "%s" }\n' % clone
if "[patch.crates-io]\n" in t:
    t = t.replace("[patch.crates-io]\n", "[patch.crates-io]\n" + line, 1)
else:
    t += "\n[patch.crates-io]\n" + line
import shutil
shutil.copyfile(manifest, manifest + ".wasm64-orig")
open(manifest, "w").write(t)
print("   patched " + manifest)
PY

echo "==> rust-src libc patched for wasm64 (build-std will use $CLONE)"
