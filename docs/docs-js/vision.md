---
title: Vision & Hearing
description: Giving models image and audio input in JavaScript
sidebar_position: 3
---

Multimodal models can take images (and audio) alongside text. This needs two things: a multimodal model plus its **mmproj** (projection) file, and an `Image` or `Audio` part passed to `ask()`.

## Loading a multimodal model

Provide the projection file with `mmprojUrl` (browser) or `mmprojPath` (Node) next to the model:

```js
const chat = await m.Chat.create({
  modelUrl: 'https://…/model.gguf',
  mmprojUrl: 'https://…/mmproj.gguf',
  contextSize: 4096, // image embeddings are large — leave room
});
```

In Node, use `modelPath` / `mmprojPath` instead.

## Images

Wrap raw image bytes with `Image.fromBytes(uint8)`, then pass an array of parts to `ask()`:

```js
// Browser: bytes from a fetch, a File, an <input type="file">, etc.
const bytes = new Uint8Array(await (await fetch('/cat.png')).arrayBuffer());
const image = m.Image.fromBytes(bytes);

const reply = await chat.ask(['What animal is in this image?', image]).completed();
```

In Node you can also load straight from disk with `m.Image.fromPath('/path/to/cat.png')`. Formats are sniffed from the file header (PNG, JPEG, etc.).

## Audio

Audio works the same way, with `Audio.fromBytes(uint8)` (or `Audio.fromPath(...)` in Node):

```js
const audio = m.Audio.fromBytes(wavBytes);
const reply = await chat.ask(['Transcribe this clip.', audio]).completed();
```

Audio decoding supports WAV, MP3, and FLAC.

:::info
Image embeddings consume a lot of context. If you hit memory limits in the browser (wasm32 caps total memory at 4 GiB), use a smaller model/mmproj or budget a `contextSize` tuned to your image sizes.
:::
