import { PromptPart } from "../generated/ts/nobodywho";

/** A part of a multimodal prompt. Construct via `Prompt.Text` / `Image` /
 *  `ImageBytes` / `Audio` / `AudioPcm`. */
class Part {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(inner: PromptPart) {
    this._inner = inner;
  }
}

/** Convert a Uint8Array (or ArrayBuffer) into a fresh ArrayBuffer holding
 *  exactly its bytes — matches what the uniffi-react-native binding expects. */
function toArrayBuffer(bytes: Uint8Array | ArrayBuffer): ArrayBuffer {
  if (bytes instanceof ArrayBuffer) return bytes;
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  );
}

/**
 * A multimodal prompt composed of text, image, and audio parts.
 *
 * `Image` / `Audio` reference files on disk. `ImageBytes` accepts encoded
 * image bytes already in memory (PNG/JPEG/etc.). `AudioPcm` accepts 16-bit
 * PCM samples + sample rate (typically 16 kHz, the default).
 *
 * @example
 * ```typescript
 * const prompt = new Prompt([
 *   Prompt.Text("Describe the image."),
 *   Prompt.Image("./dog.png"),
 *   Prompt.ImageBytes(pngBytes),
 *   Prompt.AudioPcm(pcmSamples, 16000),
 * ]);
 *
 * const stream = chat.ask(prompt);
 * ```
 */
export class Prompt {
  /** @internal */
  readonly _parts: PromptPart[];

  constructor(parts: Part[]) {
    this._parts = parts.map((p) => p._inner);
  }

  /** Create a text part. */
  static Text(content: string): Part {
    return new Part(new PromptPart.Text({ content }));
  }

  /** Create an image part from a file-system path. */
  static Image(path: string): Part {
    return new Part(new PromptPart.Image({ path }));
  }

  /** Create an image part from in-memory encoded bytes (PNG, JPEG, etc.). */
  static ImageBytes(bytes: Uint8Array | ArrayBuffer): Part {
    return new Part(new PromptPart.ImageBytes({ data: toArrayBuffer(bytes) }));
  }

  /** Create an audio part from a file-system path (WAV/MP3/FLAC). */
  static Audio(path: string): Part {
    return new Part(new PromptPart.Audio({ path }));
  }

  /**
   * Create an audio part from 16-bit PCM samples + sample rate. Every
   * current audio-capable multimodal LLM expects **16 kHz** — resample
   * before passing in. Throws at `ask()` time if the rate doesn't match.
   */
  static AudioPcm(
    samples: Int16Array | number[],
    sampleRate: number = 16000,
  ): Part {
    const samplesArr =
      samples instanceof Int16Array ? Array.from(samples) : samples;
    return new Part(
      new PromptPart.AudioPcm({ samples: samplesArr, sampleRate }),
    );
  }
}
