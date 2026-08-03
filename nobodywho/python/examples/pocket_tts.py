# /// script
# requires-python = ">=3.10"
# dependencies = ["nobodywho"]
#
# [tool.uv.sources]
# nobodywho = { path = ".." }
# ///

"""Generate a WAV file with Pocket TTS.

Run from this directory:
    uv run --script pocket_tts.py

Set HF_TOKEN after accepting the kyutai/pocket-tts terms on Hugging Face.
"""

from pathlib import Path

from nobodywho import Tts


OUTPUT_PATH = Path("pocket_tts.wav")


def main() -> None:
    tts = Tts(
        source="hf://KevinAHM/pocket-tts-onnx",
        voice="alba",
        language="english_2026-04",
        precision="int8",
    )
    wav = tts.synthesize(text="Hello from Pocket TTS and NobodyWho.")
    OUTPUT_PATH.write_bytes(wav)
    print(f"Wrote {OUTPUT_PATH.resolve()} ({len(wav)} bytes)")


if __name__ == "__main__":
    main()
