import type { RustSTTInterface } from "../generated/ts/nobodywho";
import * as nobodywho from "../generated/ts/nobodywho";
import { TokenStream } from "./streaming";

/**
 * Speech-to-text using a local Whisper ONNX model.
 *
 * @example
 * ```typescript
 * const stt = new STT("onnx-community/whisper-base");
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
  private readonly _inner: RustSTTInterface;

  /**
   * @param source - HuggingFace repo ID (e.g. `"onnx-community/whisper-base"`)
   *   or a local directory path. The model is downloaded on first use.
   * @param language - ISO 639-1 language code (e.g. `"en"`).
   *   Omit or pass `undefined` for automatic language detection.
   * @param quantization - ONNX precision variant to download and load: one of
   *   `"default"`, `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`, `"q4f16"`.
   *   Omit or pass `undefined` to use `"default"`.
   */
  constructor(source: string, language?: string, quantization?: string) {
    this._inner = new nobodywho.RustSTT(
      source,
      language ?? undefined,
      quantization ?? undefined,
    );
  }

  /**
   * Transcribe an audio file (WAV / MP3 / FLAC).
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
