# `@napi-rs/wasm-tools`

![https://github.com/napi-rs/wasm-tools/actions](https://github.com/napi-rs/wasm-tools/workflows/CI/badge.svg)

> https://github.com/rustwasm/walrus bindings

## Install this package

```
pnpm add @napi-rs/wasm-tools -D
yarn add @napi-rs/wasm-tools -D
```

## Usage

```ts
// Generate dwarf info and emit wasm

import { join } from 'node:path'
import { readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import { ModuleConfig } from '@napi-rs/wasm-tools'

const __dirname = join(fileURLToPath(import.meta.url), '..')

const wasm = await readFile(join(__dirname, 'panic.wasm32-wasi.wasm'))

const binary = new ModuleConfig()
  .generateDwarf(true)
  .generateNameSection(true)
  .generateProducersSection(true)
  .preserveCodeTransform(true)
  .parse(wasm)
  .emitWasm(true)

await writeFile(join(__dirname, 'panic.wasm32-wasi.wasm'), binary)
```
