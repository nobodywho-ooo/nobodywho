from pathlib import Path

import nobodywho


LANGUAGE = "en"
OUTPUT_PATH = Path("out.wav")


def main() -> None:
    tts = nobodywho.Tts(
        source="Supertone/supertonic-3",
        backend="supertonic",
        language=LANGUAGE,
    )
    wav = tts.synthesize(text="Hello from NobodyWho!")
    OUTPUT_PATH.write_bytes(wav)
    print(f"Wrote {OUTPUT_PATH} ({len(wav)} bytes)")


if __name__ == "__main__":
    main()
