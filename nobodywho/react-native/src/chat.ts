import {
  RustChat,
  SamplerConfig,
  MtpConfig,
  type ChatStats,
} from "../generated/ts/nobodywho";
import { Model } from "./model";
import { type Message, fromInternal, toInternal } from "./message";
import { TokenStream } from "./streaming";
import type { Prompt } from "./prompt";
import type { Tool } from "./tool";

/**
 * Settings to apply before a `complete` turn. An omitted field keeps what the
 * chat has; a set one stays set, like a leading system message.
 */
export type Options = {
  sampler?: SamplerConfig;
  /** Replaces the chat's template variables wholesale. */
  templateVariables?: Record<string, boolean>;
  /** Re-selects the chat template, so the turn re-prefills from near token zero. */
  tools?: Tool[];
};

/**
 * A chat session for local LLM inference.
 *
 * Wraps the internal RustChat with an ergonomic API that uses
 * the wrapper Tool and TokenStream types.
 *
 * @example
 * ```typescript
 * const model = await Model.load({ modelPath: "model.gguf" });
 * const chat = new Chat({
 *   model,
 *   systemPrompt: "You are a helpful assistant.",
 * });
 * for await (const token of chat.ask("Hello!")) {
 *   process.stdout.write(token);
 * }
 * ```
 */
export class Chat {
  /** @internal */
  private readonly _inner: RustChat;

  /**
   * `threadCount` is the number of CPU threads used for inference. Omit it to detect the
   * device's physical core count (performance cores only, on Apple silicon) — hyperthreads
   * and efficiency cores make inference slower, not faster. Lower it to leave CPU headroom
   * for the rest of the app.
   */
  constructor(opts: {
    model: Model;
    systemPrompt?: string;
    contextSize?: number;
    templateVariables?: Record<string, boolean>;
    tools?: Tool[];
    sampler?: SamplerConfig;
    mtp?: Partial<MtpConfig>;
    threadCount?: number;
  }) {
    this._inner = new RustChat(
      opts.model._inner,
      opts.systemPrompt ?? undefined,
      opts.contextSize ?? 4096,
      opts.templateVariables ? new Map(Object.entries(opts.templateVariables)) : undefined,
      opts.tools?.map((t) => t._inner) ?? undefined,
      opts.sampler ?? undefined,
      opts.mtp !== undefined ? MtpConfig.create(opts.mtp) : undefined,
      opts.threadCount ?? undefined,
    );
  }

  /**
   * Create a chat session directly from a model path.
   * Loads the model and creates the chat in one step.
   *
   * @example
   * ```typescript
   * const chat = await Chat.fromPath({
   *   modelPath: "model.gguf",
   *   systemPrompt: "You are a helpful assistant.",
   * });
   * ```
   */
  static async fromPath(opts: {
    modelPath: string;
    useGpu?: boolean;
    projectionModelPath?: string;
    draftModelPath?: string;
    systemPrompt?: string;
    contextSize?: number;
    templateVariables?: Record<string, boolean>;
    tools?: Tool[];
    sampler?: SamplerConfig;
    mtp?: Partial<MtpConfig>;
    threadCount?: number;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<Chat> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      projectionModelPath: opts.projectionModelPath,
      draftModelPath: opts.draftModelPath,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new Chat({ model, ...opts });
  }

  /** Send a text message or multimodal prompt and get a token stream for the response. */
  ask(message: string | Prompt): TokenStream {
    if (typeof message === "string") {
      return new TokenStream(this._inner.ask(message));
    }
    if (message._jsonString !== null) {
      return new TokenStream(this._inner.askWithJsonPrompt(message._jsonString!));
    }
    return new TokenStream(this._inner.askWithPrompt(message._parts!));
  }

  /**
   * Answer a full list of messages, replacing the chat history.
   *
   * The list is the whole conversation, used as given: it must be non-empty and
   * end in a user or tool message. A leading system message sets the chat's system
   * prompt; leave it out and the prompt already on the chat is kept. A later one
   * stays in the history, for the chat template to render in place. The response
   * is appended, and the next `ask` continues from there.
   *
   * `options` follows the same rule for the chat's other settings.
   */
  complete(messages: Message[], options: Options = {}): TokenStream {
    return new TokenStream(
      this._inner.complete(messages.map(toInternal), {
        sampler: options.sampler,
        templateVariables: options.templateVariables
          ? new Map(Object.entries(options.templateVariables))
          : undefined,
        tools: options.tools?.map((t) => t._inner),
      }),
    );
  }

  /** Stop the current generation. */
  stopGeneration(): void {
    this._inner.stopGeneration();
  }

  /** Reset the chat context with a new system prompt and tools. */
  async resetContext(opts?: {
    systemPrompt?: string;
    tools?: Tool[];
  }): Promise<void> {
    return this._inner.resetContext(
      opts?.systemPrompt ?? undefined,
      opts?.tools?.map((t) => t._inner) ?? undefined,
    );
  }

  /** Reset the chat history, keeping the system prompt and tools. */
  async resetHistory(): Promise<void> {
    return this._inner.resetHistory();
  }

  /** Get the current chat history as a list of messages. */
  async getChatHistory(): Promise<Message[]> {
    const internal = await this._inner.getChatHistory();
    return internal.map(fromInternal);
  }

  /** Set the chat history from a list of messages. */
  async setChatHistory(messages: Message[]): Promise<void> {
    return this._inner.setChatHistory(messages.map(toInternal));
  }

  /** Get the current system prompt. */
  async getSystemPrompt(): Promise<string | undefined> {
    return this._inner.getSystemPrompt();
  }

  /** Tokenize a text message or multimodal prompt and return the token IDs.
   * Each element is a token ID (number) for text, or null for image/audio embedding slots. */
  async tokenize(message: string | Prompt): Promise<(number | null)[]> {
    if (typeof message === "string") {
      return this._inner.tokenize(message);
    }
    return this._inner.tokenizeWithPrompt(message._parts);
  }

  /** Set the system prompt. */
  async setSystemPrompt(systemPrompt: string | undefined): Promise<void> {
    return this._inner.setSystemPrompt(systemPrompt);
  }

  /** Set the tools available to the model. */
  async setTools(tools: Tool[]): Promise<void> {
    return this._inner.setTools(tools.map((t) => t._inner));
  }

  /** Set a template variable. */
  async setTemplateVariable(name: string, value: boolean): Promise<void> {
    return this._inner.setTemplateVariable(name, value);
  }

  /** Get all template variables. */
  async getTemplateVariables(): Promise<Record<string, boolean>> {
    return Object.fromEntries(await this._inner.getTemplateVariables());
  }

  /** Set the sampler configuration. */
  async setSamplerConfig(sampler: SamplerConfig): Promise<void> {
    return this._inner.setSamplerConfig(sampler);
  }

  /** Get the current sampler configuration as a JSON string. */
  async getSamplerConfigJson(): Promise<string> {
    return this._inner.getSamplerConfigJson();
  }

  /** Get context usage statistics. */
  async getStats(): Promise<ChatStats> {
    return this._inner.getStats();
  }

  async mtpAcceptanceRate(): Promise<number | undefined> {
    return this._inner.mtpAcceptanceRate();
  }

  /**
   * Immediately free the underlying Rust resources (model context, KV cache, etc.).
   * After calling this, the Chat instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
