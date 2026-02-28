# NobodyWho Swift SDK Examples

This directory contains example code showing how to use the NobodyWho Swift SDK.

## QuickStart.swift

Basic example showing:
- Loading a GGUF model
- Creating a chat session
- Asking questions
- Getting chat history

## Running the Examples

### Option 1: Swift Package Manager

```bash
swift run QuickStart /path/to/your/model.gguf
```

### Option 2: Xcode

1. Open the Swift package in Xcode
2. Add the example as a target
3. Build and run

### Option 3: Swift Script

```bash
swift QuickStart.swift
```

## Prerequisites

- A GGUF model file (download from HuggingFace)
- Swift 5.9+
- macOS 11+ or iOS 13+

## Model Recommendations

For quick testing, try these small models:

- **Qwen 0.6B** (600MB): `Qwen_Qwen3-0.6B-Q4_K_M.gguf`
  - Download: https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF

- **TinyLlama 1.1B** (669MB): `tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf`
  - Download: https://huggingface.co/TheBloke/TinyLlama-1.1B-Chat-v1.0-GGUF

## API Overview

```swift
// Load model
let model = try loadModel(path: "model.gguf", useGpu: true)

// Configure chat
let config = ChatConfig(
    contextSize: 4096,          // Context window size
    systemPrompt: "You are..."  // Optional system prompt
)

// Create chat session
let chat = try Chat(model: model, config: config)

// Ask questions (blocking)
let response = try chat.askBlocking(prompt: "Hello!")

// Get history
let history = try chat.history()
for message in history {
    print("\(message.role): \(message.content)")
}
```

## Error Handling

```swift
do {
    let model = try loadModel(path: "model.gguf", useGpu: true)
    let chat = try Chat(model: model, config: config)
    let response = try chat.askBlocking(prompt: "Hi!")
} catch let error as NobodyWhoError {
    switch error {
    case .ModelNotFound:
        print("Model file not found")
    case .InvalidModel:
        print("Invalid or corrupt model")
    case .InitializationError:
        print("Failed to initialize")
    case .InferenceError:
        print("Inference failed")
    case .Other:
        print("Other error occurred")
    }
}
```

## Types

### Message
```swift
struct Message {
    let role: Role
    let content: String
}
```

### Role
```swift
enum Role {
    case user
    case assistant
    case system
    case tool
}
```

### ChatConfig
```swift
struct ChatConfig {
    let contextSize: UInt32
    let systemPrompt: String?
}
```
