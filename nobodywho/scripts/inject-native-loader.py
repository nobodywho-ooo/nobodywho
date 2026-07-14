#!/usr/bin/env python3
"""Idempotently inject the NativeLoader hook into the generated UniFFI Kotlin bindings.

uniffi-bindgen overwrites `kotlin/common/generated/uniffi/nobodywho/nobodywho.kt` on every
regeneration, dropping our hook. Under dynamic-link the binding lib depends on co-located
ggml/llama siblings JNA does not stage on its own (see NativeLoader.kt), so
`ai.nobodywho.NativeLoader.ensureLoaded()` must run before `Native.register`. uniffi 0.30's
Kotlin backend has no template-override hook, so re-injecting after generation is the
supported path (kept honest by NativeLoaderGuardTest).

The hook goes in as the first statement of every `Native.register(...)` block; a register
already preceded by it is skipped, so running twice never duplicates the line.

Usage: inject-native-loader.py <path-to-nobodywho.kt>
"""
import re
import sys

HOOK = "ai.nobodywho.NativeLoader.ensureLoaded()"
REGISTER = re.compile(
    r'^(?P<indent>[ \t]*)Native\.register\(\w+::class\.java, '
    r'findLibraryName\(componentName = "nobodywho"\)\)',
    re.MULTILINE,
)


def inject(text: str) -> str:
    out, last = [], 0
    for m in REGISTER.finditer(text):
        line_start = text.rfind("\n", 0, m.start()) + 1
        prev_line = text[text.rfind("\n", 0, line_start - 1) + 1 : line_start]
        out.append(text[last : m.start()])
        if HOOK not in prev_line:  # idempotent: skip if already injected
            out.append(f"{m.group('indent')}{HOOK}\n")
        out.append(m.group(0))
        last = m.end()
    out.append(text[last:])
    return "".join(out)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <nobodywho.kt>", file=sys.stderr)
        return 2
    path = sys.argv[1]
    with open(path, encoding="utf-8") as f:
        original = f.read()
    patched = inject(original)
    if patched != original:
        with open(path, "w", encoding="utf-8") as f:
            f.write(patched)
    return 0


if __name__ == "__main__":
    sys.exit(main())
