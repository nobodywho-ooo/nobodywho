# `@napi-rs/tar`

![https://github.com/napi-rs/tar/actions](https://github.com/napi-rs/tar/workflows/CI/badge.svg)
[![install size](https://packagephobia.com/badge?p=@napi-rs/tar)](https://packagephobia.com/result?p=@napi-rs/tar)
[![Downloads](https://img.shields.io/npm/dm/@napi-rs/tar.svg?sanitize=true)](https://npmcharts.com/compare/@napi-rs/tar?minimal=true)

> Node.js tar binding https://docs.rs/tar/latest/tar/

## Usage

### Reading Archives

```ts
export class Entries {
  [Symbol.iterator](): Iterator<Entry, void, void>
}
export class Entry {
  path(): string | null
  asBytes(): Buffer
}
export class Archive {
  /** Create a new archive with the underlying path. */
  constructor(path: string)
  entries(): Entries
  /**
   * Unpacks the contents tarball into the specified `dst`.
   *
   * This function will iterate over the entire contents of this tarball,
   * extracting each file in turn to the location specified by the entry's
   * path name.
   *
   * This operation is relatively sensitive in that it will not write files
   * outside of the path specified by `dst`. Files in the archive which have
   * a '..' in their path are skipped during the unpacking process.
   */
  unpack(to: string): void
}
```

### Creating Archives

```ts
export class Builder {
  /** Create a new builder which will write to the specified output. */
  constructor(output?: string)
  /** Append a file from disk to this archive. */
  appendFile(name: string, src: string): void
  /** Append a directory and all of its contents to this archive. */
  appendDirAll(name: string, src: string): void
  /** Append raw data to this archive with the specified name. */
  appendData(name: string, data: Uint8Array): void
  /** Finalize the archive and return the resulting data. */
  finish(): Array<number> | null
}
```

### Creating Archives Example

```ts
import { Builder } from '@napi-rs/tar'

// Create archive in memory
const builder = new Builder()
builder.appendData('hello.txt', Buffer.from('Hello, world!'))
builder.appendFile('package.json', './package.json')
builder.appendDirAll('src', './src')

const archiveData = builder.finish() // Returns Uint8Array
// archiveData can be written to disk or used directly

// Create archive to file
const fileBuilder = new Builder('./output.tar')
fileBuilder.appendData('readme.txt', Buffer.from('Archive contents'))
fileBuilder.finish() // Returns null, data written to ./output.tar
```

## Extract Single File

You can extract a specific file from a tar archive without extracting the entire archive. This is useful for inspecting Docker OCI images or extracting specific configuration files:

```ts
import { Archive } from '@napi-rs/tar'

// Extract a single file (similar to: tar -x -O -f archive.tar filename)
function extractFile(archivePath: string, targetPath: string): Buffer | null {
  const archive = new Archive(archivePath)
  for (const entry of archive.entries()) {
    if (entry.path() === targetPath) {
      return entry.asBytes()
    }
  }
  return null
}

// Usage example
const indexContent = extractFile('./docker-image.tar', 'index.json')
if (indexContent) {
  const manifest = JSON.parse(indexContent.toString('utf-8'))
  console.log(manifest)
}
```

## Install this test package

```
yarn add @napi-rs/tar
pnpm install @napi-rs/tar
npm install @napi-rs/tar
```
