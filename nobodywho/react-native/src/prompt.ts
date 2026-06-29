import { PromptPart } from "../generated/ts/nobodywho";

/** A text part of a multimodal prompt. */
class TextPart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(content: string) {
    this._inner = new PromptPart.Text({ content });
  }
}

/** An image part of a multimodal prompt. */
class ImagePart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(path: string) {
    this._inner = new PromptPart.Image({ path });
  }
}

/** An audio part of a multimodal prompt. */
class AudioPart {
  /** @internal */
  readonly _inner: PromptPart;

  constructor(path: string) {
    this._inner = new PromptPart.Audio({ path });
  }
}

type Part = TextPart | ImagePart | AudioPart;

/**
 * A multimodal prompt composed of text, image, and audio parts.
 *
 * @example
 * ```typescript
 * const prompt = new Prompt([
 *   Prompt.Text("Tell me what you see in the first image."),
 *   Prompt.Image("./dog.png"),
 *   Prompt.Text("Also tell me what you see in the second image."),
 *   Prompt.Image("./penguin.png"),
 * ]);
 *
 * const stream = chat.ask(prompt);
 *
 * // JSON-structured prompt
 * const jsonPrompt = Prompt.fromJson({ role: "user", content: "Hello" });
 * const stream = chat.ask(jsonPrompt);
 * ```
 */
export class Prompt {
  /** @internal */
  readonly _parts: PromptPart[] | null;
  /** @internal */
  readonly _jsonString: string | null;

  constructor(parts: Part[]) {
    this._parts = parts.map((p) => p._inner);
    this._jsonString = null;
  }

  /** Create a text part. */
  static Text(content: string): TextPart {
    return new TextPart(content);
  }

  /** Create an image part from a file path. */
  static Image(path: string): ImagePart {
    return new ImagePart(path);
  }

  /** Create an audio part from a file path. */
  static Audio(path: string): AudioPart {
    return new AudioPart(path);
  }

  /** Create a prompt from any JSON-serializable value (object, array, string, etc.). */
  static fromJson(data: unknown): Prompt {
    const p: Prompt = Object.create(Prompt.prototype);
    Object.assign(p, { _parts: null, _jsonString: JSON.stringify(data) });
    return p;
  }
}
