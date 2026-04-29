# `@napi-rs/lzma`

![https://github.com/Brooooooklyn/lzma/actions](https://github.com/Brooooooklyn/lzma/workflows/CI/badge.svg)
![](https://img.shields.io/npm/dm/@napi-rs/lzma.svg?sanitize=true)
[![Install size](https://packagephobia.com/badge?p=@napi-rs/lzma)](https://packagephobia.com/result?p=@napi-rs/lzma)

[lzma-rs](https://docs.rs/lzma-rs) binding to Node.js via [napi-rs](https://napi.rs).

> 🚀 Help me to become a full-time open-source developer by [sponsoring me on Github](https://github.com/sponsors/Brooooooklyn)

## Install

```
yarn add @napi-rs/lzma
```

## Support matrix

|                       | node14 | node16 | node18 | node20 |
| --------------------- | ------ | ------ | ------ | ------ |
| Windows x64           | ✓      | ✓      | ✓      | ✓      |
| Windows x32           | ✓      | ✓      | ✓      | ✓      |
| Windows arm64         | ✓      | ✓      | ✓      | ✓      |
| macOS x64             | ✓      | ✓      | ✓      | ✓      |
| macOS arm64 (m chips) | ✓      | ✓      | ✓      | ✓      |
| Linux x64 gnu         | ✓      | ✓      | ✓      | ✓      |
| Linux x64 musl        | ✓      | ✓      | ✓      | ✓      |
| Linux arm gnu         | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 gnu       | ✓      | ✓      | ✓      | ✓      |
| Linux arm64 musl      | ✓      | ✓      | ✓      | ✓      |
| Android arm64         | ✓      | ✓      | ✓      | ✓      |
| Android armv7         | ✓      | ✓      | ✓      | ✓      |
| FreeBSD x64           | ✓      | ✓      | ✓      | ✓      |

## API

### xz

```js
import { compress, decompress } from '@napi-rs/lzma/xz'

const compressed = await compress('Hello napi-rs 🚀')

const decompressed = await decompress(compressed)

console.log(decompressed.toString('utf8')) // Hello napi-rs 🚀
```

### lzma

```js
import { compress, decompress } from '@napi-rs/lzma/lzma'

const compressed = await compress('Hello napi-rs 🚀')

const decompressed = await decompress(compressed)

console.log(decompressed.toString('utf8')) // Hello napi-rs 🚀
```

### lzma2

```js
import { compress, decompress } from '@napi-rs/lzma/lzma2'

const compressed = await compress('Hello napi-rs 🚀')

const decompressed = await decompress(compressed)

console.log(decompressed.toString('utf8')) // Hello napi-rs 🚀
```
