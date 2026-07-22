from pathlib import Path

import nobodywho


OUTPUT_DIR = Path("tts_outputs")
SOURCES = {
    "kokoro": "hf://hexgrad/Kokoro-82M",
    "pocket-tts": "hf://KevinAHM/pocket-tts-onnx",
    "supertonic": "hf://Supertone/supertonic-3",
}
TEXT = "This audio file was generated completely on-device."


def main() -> None:
    OUTPUT_DIR.mkdir(exist_ok=True)

    for name, source in SOURCES.items():
        tts = nobodywho.Tts(source=source)
        wav = tts.synthesize(text=TEXT)
        output_path = OUTPUT_DIR / f"{name}.wav"
        output_path.write_bytes(wav)
        print(f"Wrote {output_path} ({len(wav)} bytes)")


if __name__ == "__main__":
    main()
