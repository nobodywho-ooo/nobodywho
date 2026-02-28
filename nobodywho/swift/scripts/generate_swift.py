#!/usr/bin/env python3
"""
Generate Swift bindings from UniFFI UDL file.

This script uses the uniffi Python package to generate Swift bindings.
Install with: pip3 install uniffi-bindgen
"""

import subprocess
import sys
from pathlib import Path

def main():
    # Paths
    script_dir = Path(__file__).parent
    swift_dir = script_dir.parent
    workspace_dir = swift_dir.parent
    core_dir = workspace_dir / "core"
    udl_file = core_dir / "src" / "nobodywho.udl"
    output_dir = swift_dir / "Sources" / "NobodyWho" / "Generated"

    # Find a built library to use for generation
    lib_path = workspace_dir / "target" / "aarch64-apple-darwin" / "release" / "libnobodywho.dylib"

    if not lib_path.exists():
        print(f"Error: Library not found at {lib_path}")
        print("Please build the library first:")
        print("  cargo build -p nobodywho --features uniffi --target aarch64-apple-darwin --release")
        return 1

    if not udl_file.exists():
        print(f"Error: UDL file not found at {udl_file}")
        return 1

    # Create output directory
    output_dir.mkdir(parents=True, exist_ok=True)

    print(f"Generating Swift bindings...")
    print(f"  UDL: {udl_file}")
    print(f"  Library: {lib_path}")
    print(f"  Output: {output_dir}")

    # Try using uniffi-bindgen Python package
    try:
        result = subprocess.run([
            "uniffi-bindgen",
            "generate",
            "--library", str(lib_path),
            "--language", "swift",
            "--out-dir", str(output_dir),
            str(udl_file)
        ], check=True, capture_output=True, text=True)

        print("\n✓ Swift bindings generated successfully!")
        print(f"\nGenerated files in {output_dir}:")
        for file in output_dir.iterdir():
            if file.is_file():
                print(f"  - {file.name}")

        return 0

    except FileNotFoundError:
        print("\nError: uniffi-bindgen not found")
        print("\nInstall it with:")
        print("  pip3 install uniffi-bindgen")
        print("\nOr use cargo:")
        print("  cargo install uniffi-bindgen --git https://github.com/mozilla/uniffi-rs")
        return 1
    except subprocess.CalledProcessError as e:
        print(f"\nError generating bindings: {e}")
        print(f"stdout: {e.stdout}")
        print(f"stderr: {e.stderr}")
        return 1

if __name__ == "__main__":
    sys.exit(main())
