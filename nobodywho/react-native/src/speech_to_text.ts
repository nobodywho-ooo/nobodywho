import { RustSpeechToText, loadSpeechToText } from "../generated/ts/nobodywho";
import { TokenStream } from "./streaming";

export class SpeechToText {
  private constructor(private readonly _stt: RustSpeechToText) {}

  static async fromPath({
    modelPath,
    language,
    translate = false,
    initialPrompt,
  }: {
    modelPath: string;
    language?: string;
    translate?: boolean;
    initialPrompt?: string;
  }): Promise<SpeechToText> {
    const stt = await loadSpeechToText(
      modelPath,
      language ?? null,
      translate,
      initialPrompt ?? null
    );
    return new SpeechToText(stt);
  }

  transcribe(audioPath: string): TokenStream {
    return new TokenStream(this._stt.transcribe(audioPath));
  }
}
