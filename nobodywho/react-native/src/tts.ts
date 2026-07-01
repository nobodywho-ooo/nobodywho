import {
  RustTts,
  loadTts,
  type RustTtsInterface,
} from "../generated/ts/nobodywho";

export type TtsBackend = "kokoro" | "supertonic";
export type TtsDevice = "auto" | "cpu" | "cuda";

export type TtsOptions = {
  source: string;
  backend?: TtsBackend;
  voice?: string;
  language?: string;
  speed?: number;
  steps?: number;
  silenceDuration?: number;
  device?: TtsDevice;
};

/** Text-to-speech synthesizer that returns WAV bytes. */
export class Tts {
  /** @internal */
  private _inner: RustTtsInterface;

  /** Create a TTS synthesizer synchronously. */
  constructor(opts: TtsOptions) {
    this._inner = new RustTts(
      opts.source,
      opts.backend,
      opts.voice,
      opts.language,
      opts.speed,
      opts.steps,
      opts.silenceDuration,
      opts.device ?? "auto",
    );
  }

  private static fromInner(inner: RustTtsInterface): Tts {
    const tts = Object.create(Tts.prototype) as Tts;
    tts._inner = inner;
    return tts;
  }

  /** Create a TTS synthesizer asynchronously. */
  static async load(opts: TtsOptions): Promise<Tts> {
    const inner = await loadTts(
      opts.source,
      opts.backend,
      opts.voice,
      opts.language,
      opts.speed,
      opts.steps,
      opts.silenceDuration,
      opts.device ?? "auto",
    );
    return Tts.fromInner(inner);
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
   * After calling this, the Tts instance is no longer usable.
   */
  destroy(): void {
    (this._inner as any).uniffiDestroy();
  }
}
