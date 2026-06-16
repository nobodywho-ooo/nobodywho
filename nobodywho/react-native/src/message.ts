import {
  Message as InternalMessage,
  Message_Tags,
  type Asset,
  type ToolCall,
} from "../generated/ts/nobodywho";

/**
 * A chat message. The variant determines the message type:
 *
 * - **User message:** `{ role: "user", content, assets? }`
 * - **Assistant message:** `{ role: "assistant", content }`
 * - **Assistant tool call:** `{ role: "assistant", content, toolCalls }`
 * - **System message:** `{ role: "system", content }`
 * - **Tool response:** `{ role: "tool", name, content }`
 *
 * @example
 * ```typescript
 * const history = await chat.getChatHistory();
 * for (const msg of history) {
 *   if (msg.role === "tool") {
 *     console.log("Tool response:", msg.name, msg.content);
 *   } else if (msg.role === "assistant" && "toolCalls" in msg) {
 *     console.log("Tool calls:", msg.toolCalls);
 *   } else {
 *     console.log(msg.role, msg.content);
 *   }
 * }
 * ```
 */
export type Message =
  | { role: "user"; content: string; assets?: Asset[] }
  | { role: "assistant"; content: string }
  | { role: "assistant"; content: string; toolCalls: ToolCall[] }
  | { role: "system"; content: string }
  | { role: "tool"; name: string; content: string };

/** @internal Convert internal Message to Message */
export function fromInternal(msg: InternalMessage): Message {
  if (msg.tag === Message_Tags.User) {
    const { content, assets } = msg.inner;
    return {
      role: "user",
      content,
      ...(assets.length > 0 ? { assets } : {}),
    };
  } else if (msg.tag === Message_Tags.Assistant) {
    const { content, toolCalls } = msg.inner;
    if (toolCalls != null && toolCalls.length > 0) {
      return { role: "assistant", content, toolCalls };
    }
    return { role: "assistant", content };
  } else if (msg.tag === Message_Tags.System) {
    const { content } = msg.inner;
    return { role: "system", content };
  } else {
    const { name, content } = msg.inner;
    return { role: "tool", name, content };
  }
}

/** @internal Convert Message to internal Message */
export function toInternal(msg: Message): InternalMessage {
  if (msg.role === "user") {
    return new InternalMessage.User({
      content: msg.content,
      assets: msg.assets ?? [],
    });
  } else if (msg.role === "assistant" && "toolCalls" in msg) {
    return new InternalMessage.Assistant({
      content: msg.content,
      toolCalls: msg.toolCalls,
    });
  } else if (msg.role === "assistant") {
    return new InternalMessage.Assistant({
      content: msg.content,
      toolCalls: undefined,
    });
  } else if (msg.role === "system") {
    return new InternalMessage.System({
      content: msg.content,
    });
  } else {
    return new InternalMessage.Tool({
      name: msg.name,
      content: msg.content,
    });
  }
}
