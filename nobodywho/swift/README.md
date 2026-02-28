# NobodyWho Swift SDK

Native Swift SDK for running LLMs locally on iOS and macOS.

## Features

- 🍎 Native Swift API for iOS 13+ and macOS 11+
- ⚡ GPU-accelerated inference using Metal
- 🔒 100% local - no internet required
- 📦 Distributed as Swift Package or XCFramework
- 🦀 Powered by Rust core with UniFFI bindings

## Installation

### Swift Package Manager

Add this package to your `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/nobodywho-ooo/nobodywho", from: "0.1.0")
]
```

Or in Xcode:
1. File → Add Package Dependencies
2. Enter: `https://github.com/nobodywho-ooo/nobodywho`
3. Select version `0.1.0` or later

**That's it!** SPM automatically downloads the pre-built XCFramework from GitHub releases.

### Local Development

For contributors building from source:

```bash
# Clone the repository
git clone https://github.com/nobodywho-ooo/nobodywho
cd nobodywho/swift

# Build the XCFramework
./scripts/build_xcframework.sh
```

## Usage

```swift
import NobodyWho

// Load a model
let model = try loadModel(
    path: "/path/to/model.gguf",
    useGpu: true
)

// Create a chat
let config = ChatConfig(
    contextSize: 4096,
    systemPrompt: "You are a helpful assistant"
)
let chat = try Chat(model: model, config: config)

// Ask questions
let response = try chat.askBlocking(prompt: "Hello!")
print(response)

// Get chat history
let history = try chat.history()
for message in history {
    print("\(message.role): \(message.content)")
}
```

## Quick Start

### 1. XCFramework is Ready ✅

The XCFramework has been built and is located at:
```
swift/NobodyWhoFFI.xcframework
```

### 2. Generate Swift Bindings

See [BINDINGS.md](BINDINGS.md) for instructions on generating Swift bindings.

## Building from Source

### Requirements

- Rust 1.70+ with targets:
  - `aarch64-apple-ios`
  - `aarch64-apple-ios-sim`
  - `x86_64-apple-ios`
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
- Xcode 14+
- macOS 11+

### Build XCFramework

```bash
cd swift
./scripts/build_xcframework.sh
```

For debug builds:
```bash
./scripts/build_xcframework.sh --debug
```

### Environment Variables

- `NOBODYWHO_SWIFT_XCFRAMEWORK_PATH`: Override XCFramework location
- `BUILD_TYPE`: Set to `debug` or `release` (default: `release`)

## Architecture

```
NobodyWho Swift SDK
├─ Rust Core (nobodywho)
│  └─ UniFFI bindings
├─ Generated Swift bindings
└─ Swift wrapper (ergonomic API)
```

The SDK uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to automatically generate Swift bindings from the Rust core library.

## API Reference

### Model

```swift
func loadModel(path: String, useGpu: Bool) throws -> Model
```

Load a GGUF model from disk.

### Chat

```swift
class Chat {
    init(model: Model, config: ChatConfig) throws
    func askBlocking(prompt: String) throws -> String
    func history() throws -> [Message]
}
```

Main chat interface for conversational interactions.

### ChatConfig

```swift
struct ChatConfig {
    let contextSize: UInt32
    let systemPrompt: String?
}
```

Configuration for chat sessions.

### Message

```swift
struct Message {
    let role: Role
    let content: String
}

enum Role {
    case user
    case assistant
    case system
    case tool
}
```

Chat message types.

## Distribution

The XCFramework binary is **not committed** to git. Instead:

- **Users**: SPM automatically downloads from GitHub releases
- **CI**: Automatically builds and publishes to GitHub releases
- **Contributors**: Build locally with `./scripts/build_xcframework.sh`

See [RELEASE.md](RELEASE.md) for release process details.

## License

Same license as the main NobodyWho project (EUPL-1.2).
