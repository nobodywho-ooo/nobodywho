import { PromptPart } from "../generated/ts/nobodywho";

/** A text part of a multimodal prompt. */
class TextPart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(content: string) {
    this._inner = new PromptPart.Text({ content });
  }
}

/** An image part: file path or in-memory encoded bytes. */
class ImagePart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(source: string | Uint8Array | ArrayBuffer) {
    if (typeof source === "string") {
      this._inner = new PromptPart.Image({ path: source });
    } else {
      // Normalize to a fresh ArrayBuffer covering exactly the view's bytes —
      // matches what the uniffi-react-native binding expects.
      const buffer =
        source instanceof ArrayBuffer
          ? source
          : source.buffer.slice(
              source.byteOffset,
              source.byteOffset + source.byteLength,
            );
      this._inner = new PromptPart.ImageBytes({ data: buffer });
    }
  }
}

/** An audio part: file path or in-memory 16-bit PCM samples + sample rate. */
class AudioPart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(
    source: string | { samples: Int16Array | number[]; sampleRate?: number },
  ) {
    if (typeof source === "string") {
      this._inner = new PromptPart.Audio({ path: source });
    } else {
      const samplesArr =
        source.samples instanceof Int16Array
          ? Array.from(source.samples)
          : source.samples;
      this._inner = new PromptPart.AudioPcm({
        samples: samplesArr,
        sampleRate: source.sampleRate ?? 16000,
      });
    }
  }
}

type Part = TextPart | ImagePart | AudioPart;

/**
 * A multimodal prompt composed of text, image, and audio parts.
 *
 * Image and audio parts can be supplied as a file-system path or as in-memory
 * content (encoded image bytes / 16-bit PCM audio samples). Every current
 * audio-capable multimodal LLM expects **16 kHz** PCM — the default if you
 * omit `sampleRate`.
 *
 * @example
 * ```typescript
 * const prompt = new Prompt([
 *   Prompt.Text("Describe the image."),
 *   Prompt.Image("./dog.png"),                                 // from disk
 *   Prompt.Image(pngBytes),                                    // in memory
 *   Prompt.Audio({ samples: pcmSamples, sampleRate: 16000 }),  // mic capture
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
  static Text(content: string): TextPart {
    return new TextPart(content);
  }

  /** Create an image part: file path, `Uint8Array`, or `ArrayBuffer`. */
  static Image(source: string | Uint8Array | ArrayBuffer): ImagePart {
    return new ImagePart(source);
  }

  /**
   * Create an audio part: file path (string), or PCM samples object
   * `{ samples, sampleRate? }`.
   */
  static Audio(
    source: string | { samples: Int16Array | number[]; sampleRate?: number },
  ): AudioPart {
    return new AudioPart(source);
  }
}
