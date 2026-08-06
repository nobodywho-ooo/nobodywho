import type { RustVadInterface, VadEvent } from "../generated/ts/nobodywho";
import * as nobodywho from "../generated/ts/nobodywho";

export { VadEvent } from "../generated/ts/nobodywho";

export type VadOptions = {
  source?: string;
  sampleRate: number;
  threshold?: number;
  minSilenceDurationMs?: number;
  minSpeechDurationMs?: number;
  prerollDurationMs?: number;
  device?: "auto" | "cpu" | "cuda";
};

/**
 * Voice activity detection from live, streaming audio, backed by Silero VAD.
 *
 * @example
 * ```typescript
 * const vad = new Vad({ sampleRate: 16000 });
 *
 * // Feed each newest chunk as it arrives (not the whole buffer — Vad
 * // tracks the current turn internally).
 * const event = vad.push(chunk);
 * if (event === VadEvent.SpeechEnded) {
 *   const audio = vad.finish();
 *   // audio: Int16Array-like number[] spanning SpeechStarted (with a
 *   // small pre-roll) through SpeechEnded.
 * }
 * ```
 */
export class Vad {
  /** @internal */
  private readonly _inner: RustVadInterface;

  /**
   * @param opts - See {@link VadOptions}.
   */
  constructor(opts: VadOptions) {
    this._inner = new nobodywho.RustVad(
      opts.source,
      opts.sampleRate,
      opts.threshold,
      opts.minSilenceDurationMs,
      opts.minSpeechDurationMs,
      opts.prerollDurationMs,
      opts.device,
    );
  }

  /**
   * Feed the newest chunk of audio (not the whole accumulated buffer —
   * `Vad` tracks the current turn internally). Returns a `VadEvent` if this
   * call crossed a confirmed speech/silence boundary.
   *
   * @param chunk - Flat array of signed 16-bit samples (mono).
   */
  push(chunk: Int16Array | number[]): VadEvent | undefined {
    const arr = chunk instanceof Int16Array ? Array.from(chunk) : chunk;
    return this._inner.push(arr);
  }

  /**
   * Return the current turn's captured audio (from the confirmed
   * `SpeechStarted`, including a small pre-roll, through to `SpeechEnded`)
   * and reset internal state for the next turn. Empty if speech was never
   * confirmed.
   */
  finish(): number[] {
    return this._inner.finish();
  }

  /** Release native resources. Call when done with this handle. */
  destroy(): void {
    (this._inner as any).uniffiDestroy?.();
  }
}
