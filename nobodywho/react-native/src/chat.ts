import {
  RustChat,
  SamplerConfig,
} from "../generated/ts/nobodywho";
import { Model } from "./model";
import { type ChatMessage, fromInternal, toInternal } from "./message";
import { TokenStream } from "./streaming";
import type { Prompt } from "./prompt";
import type { Tool } from "./tool";

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

  constructor(opts: {
    model: Model;
    systemPrompt?: string;
    contextSize?: number;
    templateVariables?: Record<string, boolean>;
    tools?: Tool[];
    sampler?: SamplerConfig;
  }) {
    this._inner = new RustChat(
      opts.model._inner,
      opts.systemPrompt ?? undefined,
      opts.contextSize ?? 4096,
      opts.templateVariables ? new Map(Object.entries(opts.templateVariables)) : undefined,
      opts.tools?.map((t) => t._inner) ?? undefined,
      opts.sampler ?? undefined,
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
    systemPrompt?: string;
    contextSize?: number;
    templateVariables?: Record<string, boolean>;
    tools?: Tool[];
    sampler?: SamplerConfig;
    onDownloadProgress?: (downloaded: number, total: number) => void;
  }): Promise<Chat> {
    const model = await Model.load({
      modelPath: opts.modelPath,
      useGpu: opts.useGpu,
      projectionModelPath: opts.projectionModelPath,
      onDownloadProgress: opts.onDownloadProgress,
    });
    return new Chat({ model, ...opts });
  }

  /** Send a text message or multimodal prompt and get a token stream for the response. */
  ask(message: string | Prompt): TokenStream {
    if (typeof message === "string") {
      return new TokenStream(this._inner.ask(message));
    }
    return new TokenStream(this._inner.askWithPrompt(message._parts));
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
  async getChatHistory(): Promise<ChatMessage[]> {
    const internal = await this._inner.getChatHistory();
    return internal.map(fromInternal);
  }

  /** Set the chat history from a list of messages. */
  async setChatHistory(messages: ChatMessage[]): Promise<void> {
    return this._inner.setChatHistory(messages.map(toInternal));
  }

  /** Get the current system prompt. */
  async getSystemPrompt(): Promise<string | undefined> {
    return this._inner.getSystemPrompt();
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

  /**
   * Immediately free the underlying Rust resources (model context, KV cache, etc.).
   * After calling this, the Chat instance is no longer usable.
   */
  destroy(): void {
    this._inner.uniffiDestroy();
  }
}
