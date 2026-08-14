#!/usr/bin/env python3
"""Route generated UniFFI library lookup through NativeLoader."""

import sys
from pathlib import Path

ORIGINAL = '''@Synchronized
private fun findLibraryName(componentName: String): String {
    val libOverride = System.getProperty("uniffi.component.$componentName.libraryOverride")
    if (libOverride != null) {
        return libOverride
    }
    return "nobodywho_uniffi"
}'''
PATCHED = "private fun findLibraryName(componentName: String) = NativeLoader.findLibraryName(componentName)"


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"usage: {sys.argv[0]} <nobodywho.kt>")
    path = Path(sys.argv[1])
    source = path.read_text(encoding="utf-8")
    if PATCHED not in source:
        if source.count(ORIGINAL) != 1:
            raise SystemExit("expected one generated findLibraryName function")
        source = source.replace(ORIGINAL, PATCHED)
    path.write_text(source.rstrip() + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
