import {
  ContentPart as InternalContentPart,
  ContentPart_Tags,
  Message as InternalMessage,
  Message_Tags,
  MessageContent as InternalMessageContent,
  MessageContent_Tags,
  type ToolCall,
} from "../generated/ts/nobodywho";

/** One piece of a message: a run of text, or a media file at this position. */
export type ContentPart =
  | { type: "text"; text: string }
  | { type: "image"; path: string }
  | { type: "audio"; path: string };

/**
 * Message content: either plain text, or interleaved text and media.
 *
 * @example
 * ```typescript
 * const content: Content = [
 *   { type: "text", text: "What is in this image?" },
 *   { type: "image", path: "./dog.png" },
 * ];
 * ```
 */
export type Content = string | ContentPart[];

/**
 * A chat message. The variant determines the message type:
 *
 * - **User message:** `{ role: "user", content }`
 * - **Assistant message:** `{ role: "assistant", content }`
 * - **Assistant tool call:** `{ role: "assistant", content, toolCalls }`
 * - **System message:** `{ role: "system", content }`
 * - **Tool response:** `{ role: "tool", name, content }`
 *
 * `content` is a string, or a list of parts for multimodal input.
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
  | { role: "user"; content: Content }
  | { role: "assistant"; content: Content }
  | { role: "assistant"; content: Content; toolCalls: ToolCall[] }
  | { role: "system"; content: Content }
  | { role: "tool"; name: string; content: Content };

function partFromInternal(part: InternalContentPart): ContentPart {
  if (part.tag === ContentPart_Tags.Text) {
    return { type: "text", text: part.inner.text };
  } else if (part.tag === ContentPart_Tags.Image) {
    return { type: "image", path: part.inner.path };
  } else {
    return { type: "audio", path: part.inner.path };
  }
}

function partToInternal(part: ContentPart): InternalContentPart {
  if (part.type === "text") {
    return new InternalContentPart.Text({ text: part.text });
  } else if (part.type === "image") {
    return new InternalContentPart.Image({ path: part.path });
  } else {
    return new InternalContentPart.Audio({ path: part.path });
  }
}

/** @internal */
export function contentFromInternal(content: InternalMessageContent): Content {
  if (content.tag === MessageContent_Tags.Text) {
    return content.inner.text;
  } else if (content.tag === MessageContent_Tags.Parts) {
    return content.inner.parts.map(partFromInternal);
  } else {
    // Raw JSON passthrough has no typed representation here; hand back the
    // encoded form so it survives a round trip.
    return content.inner.json;
  }
}

/** @internal */
export function contentToInternal(content: Content): InternalMessageContent {
  if (typeof content === "string") {
    return new InternalMessageContent.Text({ text: content });
  }
  return new InternalMessageContent.Parts({
    parts: content.map(partToInternal),
  });
}

/** @internal Convert internal Message to Message */
export function fromInternal(msg: InternalMessage): Message {
  if (msg.tag === Message_Tags.User) {
    return { role: "user", content: contentFromInternal(msg.inner.content) };
  } else if (msg.tag === Message_Tags.Assistant) {
    const { content, toolCalls } = msg.inner;
    if (toolCalls != null && toolCalls.length > 0) {
      return {
        role: "assistant",
        content: contentFromInternal(content),
        toolCalls,
      };
    }
    return { role: "assistant", content: contentFromInternal(content) };
  } else if (msg.tag === Message_Tags.System) {
    return { role: "system", content: contentFromInternal(msg.inner.content) };
  } else {
    const { name, content } = msg.inner;
    return { role: "tool", name, content: contentFromInternal(content) };
  }
}

/** @internal Convert Message to internal Message */
export function toInternal(msg: Message): InternalMessage {
  if (msg.role === "user") {
    return new InternalMessage.User({
      content: contentToInternal(msg.content),
    });
  } else if (msg.role === "assistant" && "toolCalls" in msg) {
    return new InternalMessage.Assistant({
      content: contentToInternal(msg.content),
      toolCalls: msg.toolCalls,
    });
  } else if (msg.role === "assistant") {
    return new InternalMessage.Assistant({
      content: contentToInternal(msg.content),
      toolCalls: undefined,
    });
  } else if (msg.role === "system") {
    return new InternalMessage.System({
      content: contentToInternal(msg.content),
    });
  } else {
    return new InternalMessage.Tool({
      name: msg.name,
      content: contentToInternal(msg.content),
    });
  }
}
