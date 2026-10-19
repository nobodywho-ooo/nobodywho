import {
  RustTextToSpeech,
  loadTextToSpeech,
  type RustTextToSpeechInterface,
} from "../generated/ts/nobodywho";

export type TextToSpeechArchitecture = "kokoro" | "pocket-tts" | "supertonic";
export type TextToSpeechDevice = "auto" | "cpu" | "cuda";

export type TextToSpeechOptions = {
  source: string;
  architecture?: TextToSpeechArchitecture;
  voice?: string;
  language?: string;
  speed?: number;
  steps?: number;
  silenceDuration?: number;
  precision?: "int8" | "fp32";
  temperature?: number;
  huggingfaceToken?: string;
  device?: TextToSpeechDevice;
};

/** Text-to-speech synthesizer that returns WAV bytes. */
export class TextToSpeech {
  /** @internal */
  private _inner: RustTextToSpeechInterface;

  /** Create a TextToSpeech synthesizer synchronously. */
  constructor(opts: TextToSpeechOptions) {
    this._inner = new RustTextToSpeech(
      opts.source,
      opts.architecture,
      opts.voice,
      opts.language,
      opts.speed,
      opts.steps,
      opts.silenceDuration,
      opts.precision,
      opts.temperature,
      opts.huggingfaceToken,
      opts.device ?? "auto",
    );
  }

  private static fromInner(inner: RustTextToSpeechInterface): TextToSpeech {
    const tts = Object.create(TextToSpeech.prototype) as TextToSpeech;
    tts._inner = inner;
    return tts;
  }

  /** Create a TextToSpeech synthesizer asynchronously. */
  static async load(opts: TextToSpeechOptions): Promise<TextToSpeech> {
    const inner = await loadTextToSpeech(
      opts.source,
      opts.architecture,
      opts.voice,
      opts.language,
      opts.speed,
      opts.steps,
      opts.silenceDuration,
      opts.precision,
      opts.temperature,
      opts.huggingfaceToken,
      opts.device ?? "auto",
    );
    return TextToSpeech.fromInner(inner);
  }

  /** Synthesize text and return WAV bytes. */
  async synthesize(text: string): Promise<Uint8Array> {
    return new Uint8Array(await this._inner.synthesizeAsync(text));
  }

  /** Synthesize text synchronously and return WAV bytes. */
  synthesizeSync(text: string): Uint8Array {
    return new Uint8Array(this._inner.synthesize(text));
  }

  /**
   * Immediately free the underlying Rust resources.
   * After calling this, the TextToSpeech instance is no longer usable.
   */
  destroy(): void {
    (this._inner as any).uniffiDestroy();
  }
}
