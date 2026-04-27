import {
  Message as InternalMessage,
  Message_Tags,
  Role,
  type Asset,
  type ToolCall,
} from "../generated/ts/nobodywho";

/**
 * A chat message. The `role` field combined with the presence of
 * `toolCalls` or `name` determines the message type:
 *
 * - **User/Assistant/System message:** `{ role, content, assets? }`
 * - **Tool call request:** `{ role: Role.Assistant, content, toolCalls }`
 * - **Tool response:** `{ role: Role.Tool, name, content }`
 *
 * @example
 * ```typescript
 * const history = await chat.getChatHistory();
 * for (const msg of history) {
 *   if ("toolCalls" in msg) {
 *     console.log("Tool calls:", msg.toolCalls);
 *   } else if ("name" in msg) {
 *     console.log("Tool response:", msg.name, msg.content);
 *   } else {
 *     console.log(msg.role, msg.content);
 *   }
 * }
 * ```
 */
export type Message =
  | { role: Role.User | Role.Assistant | Role.System; content: string; assets?: Asset[] }
  | { role: Role.Assistant; content: string; toolCalls: ToolCall[] }
  | { role: Role.Tool; name: string; content: string };

/** @internal Convert internal Message to Message */
export function fromInternal(msg: InternalMessage): Message {
  if (msg.tag === Message_Tags.Standard) {
    const { role, content, assets } = msg.inner;
    return {
      role: role as Role.User | Role.Assistant | Role.System,
      content,
      ...(assets.length > 0 ? { assets } : {}),
    };
  } else if (msg.tag === Message_Tags.ToolCalls) {
    const { role, content, toolCalls } = msg.inner;
    return { role: role as Role.Assistant, content, toolCalls };
  } else {
    const { role, name, content } = msg.inner;
    return { role: role as Role.Tool, name, content };
  }
}

/** @internal Convert Message to internal Message */
export function toInternal(msg: Message): InternalMessage {
  if ("toolCalls" in msg) {
    return new InternalMessage.ToolCalls({
      role: msg.role,
      content: msg.content,
      toolCalls: msg.toolCalls,
    });
  } else if ("name" in msg) {
    return new InternalMessage.ToolResult({
      role: msg.role,
      name: msg.name,
      content: msg.content,
    });
  } else {
    return new InternalMessage.Standard({
      role: msg.role,
      content: msg.content,
      assets: msg.assets ?? [],
    });
  }
}
