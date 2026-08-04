import type { RustSttInterface } from "../generated/ts/nobodywho";
import * as nobodywho from "../generated/ts/nobodywho";
import { TokenStream } from "./streaming";

export type SttOptions = {
  source: string;
  language?: string;
  quantization?: string;
};

/**
 * Speech-to-text using a local Whisper ONNX model.
 *
 * @example
 * ```typescript
 * const stt = await STT.load({
 *   source: "hf://onnx-community/whisper-base",
 *   language: "en",
 * });
 * for await (const piece of stt.transcribeFile("recording.mp3")) {
 *   process.stdout.write(piece);
 * }
 *
 * // From microphone (react-native-audio-pcm-stream gives Int16Array):
 * const stream = stt.transcribePcm(samplesI16, 44100);
 * const text = await stream.completed();
 * ```
 */
export class STT {
  /** @internal */
  private readonly _inner: RustSttInterface;

  /** @internal */
  private constructor(inner: RustSttInterface) {
    this._inner = inner;
  }

  /**
   * Load a Whisper STT model.
   *
   * @param opts - See {@link SttOptions}.
   */
  static async load(opts: SttOptions): Promise<STT> {
    const inner = await nobodywho.loadStt(
      opts.source,
      opts.language,
      opts.quantization,
    );
    return new STT(inner);
  }

  /**
   * Transcribe an audio file (WAV / MP3).
   * Returns an `STTStream` to consume tokens as they arrive.
   */
  transcribeFile(path: string): TokenStream {
    return new TokenStream(this._inner.transcribeFile(path));
  }

  /**
   * Transcribe raw i16 PCM samples (e.g. from a microphone).
   * The backend resamples to 16 kHz internally, so pass whatever rate your
   * mic captures at (typically 44100 or 48000).
   *
   * @param samples - Flat array of signed 16-bit samples (mono).
   * @param sampleRate - Capture rate in Hz.
   */
  transcribePcm(samples: Int16Array | number[], sampleRate: number): TokenStream {
    const arr = samples instanceof Int16Array ? Array.from(samples) : samples;
    return new TokenStream(this._inner.transcribePcm(arr, sampleRate));
  }

  /** Release native resources. Call when done with this handle. */
  destroy(): void {
    (this._inner as any).uniffiDestroy?.();
  }
}
