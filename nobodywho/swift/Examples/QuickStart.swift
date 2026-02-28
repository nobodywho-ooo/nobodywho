// QuickStart Example for NobodyWho Swift SDK
//
// This example shows how to use the NobodyWho SDK to run LLMs locally on iOS/macOS

import Foundation
import NobodyWho

func main() throws {
    // Initialize logging (optional)
    initLogging()

    // Load a model from a GGUF file
    // Replace with your actual model path
    let modelPath = "/path/to/your/model.gguf"
    let model = try loadModel(path: modelPath, useGpu: true)

    print("✓ Model loaded: \(model)")

    // Create a chat configuration
    let config = ChatConfig(
        contextSize: 4096,
        systemPrompt: "You are a helpful assistant."
    )

    // Create a chat session
    let chat = try Chat(model: model, config: config)

    print("✓ Chat session created")

    // Ask a question
    print("\nAsking: 'What is the capital of France?'")
    let response = try chat.askBlocking(prompt: "What is the capital of France?")

    print("\nResponse:")
    print(response)

    // Get chat history
    let history = try chat.history()

    print("\n--- Chat History ---")
    for message in history {
        print("[\(message.role)]: \(message.content)")
    }

    // Ask another question
    print("\nAsking: 'What is its population?'")
    let response2 = try chat.askBlocking(prompt: "What is its population?")

    print("\nResponse:")
    print(response2)
}

// Run the example
do {
    try main()
} catch {
    print("Error: \(error)")
}
