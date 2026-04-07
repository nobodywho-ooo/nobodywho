import {
  RustChat,
  type ModelInterface,
  type SamplerConfigInterface,
  type Message,
  type PromptPart,
} from "../generated/ts/nobodywho";
import { TokenStream } from "./streaming";
import type { Tool } from "./tool";

/**
 * A chat session for local LLM inference.
 *
 * Wraps the internal RustChat with an ergonomic API that uses
 * the wrapper Tool and TokenStream types.
 *
 * @example
 * ```typescript
 * const model = await loadModel("model.gguf", true);
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
    model: ModelInterface;
    systemPrompt?: string;
    contextSize?: number;
    templateVariables?: Record<string, boolean>;
    tools?: Tool[];
    sampler?: SamplerConfigInterface;
  }) {
    this._inner = new RustChat(
      opts.model,
      opts.systemPrompt ?? null,
      opts.contextSize ?? 4096,
      opts.templateVariables ?? null,
      opts.tools?.map((t) => t._inner) ?? null,
      opts.sampler ?? null,
    );
  }

  /** Send a text message and get a token stream for the response. */
  ask(message: string): TokenStream {
    return new TokenStream(this._inner.ask(message));
  }

  /** Send a multimodal prompt (text + images/audio) and get a token stream. */
  askWithPrompt(parts: PromptPart[]): TokenStream {
    return new TokenStream(this._inner.askWithPrompt(parts));
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
      opts?.systemPrompt ?? null,
      opts?.tools?.map((t) => t._inner) ?? null,
    );
  }

  /** Reset the chat history, keeping the system prompt and tools. */
  async resetHistory(): Promise<void> {
    return this._inner.resetHistory();
  }

  /** Get the current chat history as a list of messages. */
  async getChatHistory(): Promise<Message[]> {
    return this._inner.getChatHistory();
  }

  /** Set the chat history from a list of messages. */
  async setChatHistory(messages: Message[]): Promise<void> {
    return this._inner.setChatHistory(messages);
  }

  /** Get the current system prompt. */
  async getSystemPrompt(): Promise<string | null> {
    return this._inner.getSystemPrompt();
  }

  /** Set the system prompt. */
  async setSystemPrompt(systemPrompt: string | null): Promise<void> {
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
    return this._inner.getTemplateVariables();
  }

  /** Set the sampler configuration. */
  async setSamplerConfig(sampler: SamplerConfigInterface): Promise<void> {
    return this._inner.setSamplerConfig(sampler);
  }

  /** Get the current sampler configuration as a JSON string. */
  async getSamplerConfigJson(): Promise<string> {
    return this._inner.getSamplerConfigJson();
  }
}
