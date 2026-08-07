import type { RustVoiceActivityDetectionInterface, VoiceActivityDetectionEvent } from "../generated/ts/nobodywho";
import * as nobodywho from "../generated/ts/nobodywho";

export { VoiceActivityDetectionEvent } from "../generated/ts/nobodywho";

export type VoiceActivityDetectionOptions = {
  source?: string;
  sampleRate: number;
  threshold?: number;
  minSilenceDurationMs?: number;
  minSpeechDurationMs?: number;
  prerollDurationMs?: number;
};

/**
 * Voice activity detection from live, streaming audio, backed by Silero VAD.
 *
 * @example
 * ```typescript
 * const vad = await VoiceActivityDetection.load({ sampleRate: 16000 });
 *
 * // Feed each newest chunk as it arrives (not the whole buffer — VoiceActivityDetection
 * // tracks the current turn internally).
 * if (vad.push(chunk) === VoiceActivityDetectionEvent.SpeechEnded) {
 *   const audio = vad.finish();
 *   // audio: Int16Array-like number[] spanning SpeechStarted (with a
 *   // small pre-roll) through SpeechEnded.
 * }
 * ```
 */
export class VoiceActivityDetection {
  /** @internal */
  private readonly _inner: RustVoiceActivityDetectionInterface;

  /** @internal */
  private constructor(inner: RustVoiceActivityDetectionInterface) {
    this._inner = inner;
  }

  /**
   * Load a voice activity detector.
   *
   * @param opts - See {@link VoiceActivityDetectionOptions}.
   */
  static async load(opts: VoiceActivityDetectionOptions): Promise<VoiceActivityDetection> {
    const inner = await nobodywho.loadVoiceActivityDetection(
      opts.source,
      opts.sampleRate,
      opts.threshold,
      opts.minSilenceDurationMs,
      opts.minSpeechDurationMs,
      opts.prerollDurationMs,
      undefined,
    );
    return new VoiceActivityDetection(inner);
  }

  /**
   * Feed the newest chunk of audio (not the whole accumulated buffer —
   * `VoiceActivityDetection` tracks the current turn internally). Always
   * returns the current confirmed state: `Speech`/`Silence` if unchanged
   * since the last call, or `SpeechStarted`/`SpeechEnded` on the call that
   * confirmed the transition.
   *
   * @param chunk - Flat array of signed 16-bit samples (mono).
   */
  push(chunk: Int16Array | number[]): VoiceActivityDetectionEvent {
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

  /**
   * Detect every speech segment in a complete audio buffer, returning
   * each segment's audio (with a short pre-roll) in order. Unlike `push`,
   * correctly finds every segment regardless of buffer size — use this
   * for offline/batch processing instead of live streaming.
   *
   * @example
   * ```typescript
   * for (const audio of vad.segment(fullRecording)) {
   *   transcribe(audio);
   * }
   * ```
   */
  segment(samples: Int16Array | number[]): number[][] {
    const arr = samples instanceof Int16Array ? Array.from(samples) : samples;
    return this._inner.segment(arr);
  }

  /** Release native resources. Call when done with this handle. */
  destroy(): void {
    (this._inner as any).uniffiDestroy?.();
  }
}
