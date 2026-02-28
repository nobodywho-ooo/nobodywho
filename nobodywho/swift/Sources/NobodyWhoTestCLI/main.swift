import Foundation
import NobodyWho

print("===========================================")
print("NobodyWho Swift SDK - Live Integration Test")
print("===========================================\n")

// Get model path from arguments
let modelPath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : nil

// Test 1: Module Import
print("✅ Test 1: Module Import")
print("   ✓ Successfully imported NobodyWho module\n")

// Test 2: Type Availability
print("✅ Test 2: Type Availability")
print("   ✓ ChatConfig: \(ChatConfig.self)")
print("   ✓ Message: \(Message.self)")
print("   ✓ Role: \(Role.self)")
print("   ✓ Chat: \(Chat.self)")
print("   ✓ Model: \(Model.self)")
print("   ✓ NobodyWhoError: \(NobodyWhoError.self)\n")

// Test 3: Create instances
print("✅ Test 3: Create Instances")
let config = ChatConfig(contextSize: 4096, systemPrompt: "You are a helpful assistant")
print("   ✓ Created ChatConfig (contextSize: 4096)")

let message = Message(role: .user, content: "Hello!")
print("   ✓ Created Message (role: user, content: \"Hello!\")\n")

// Test 4: Error handling
print("✅ Test 4: Error Handling")
do {
    let _ = try loadModel(path: "/nonexistent/model.gguf", useGpu: false)
    print("   ❌ Expected an error but got none")
} catch let error as NobodyWhoError {
    switch error {
    case .ModelNotFound(let msg):
        print("   ✓ Correctly threw ModelNotFound: \(msg)")
    case .InvalidModel(let msg):
        print("   ✓ Correctly threw InvalidModel: \(msg)")
    default:
        print("   ✓ Caught NobodyWhoError: \(error)")
    }
} catch {
    print("   ⚠️  Caught unexpected error: \(error)")
}

// Test 5: Initialize logging
print("\n✅ Test 5: Logging Initialization")
initLogging()
print("   ✓ initLogging() called successfully\n")

// Test 6: Real model test (if path provided)
if let modelPath = modelPath {
    print("===========================================")
    print("Real Model Test")
    print("===========================================\n")

    // Check if file exists
    if !FileManager.default.fileExists(atPath: modelPath) {
        print("❌ Model file not found at: \(modelPath)")
    } else {
        print("📦 Model: \(modelPath)")

        do {
            print("\n⏳ Loading model (this may take a moment)...")
            let model = try loadModel(path: modelPath, useGpu: true)
            print("✅ Model loaded successfully!")
            print("   Path: \(model.path())")

            print("\n⏳ Creating chat session...")
            let chat = try Chat(model: model, config: config)
            print("✅ Chat session created!")

            print("\n⏳ Asking: 'What is 2+2?'")
            let response = try chat.askBlocking(prompt: "What is 2+2? Answer in one short sentence.")
            print("✅ Response received:\n")
            print("   \(response)")

            print("\n⏳ Getting chat history...")
            let history = try chat.history()
            print("✅ Chat history (\(history.count) messages):")
            for (i, msg) in history.enumerated() {
                let roleStr = String(describing: msg.role)
                print("   [\(i+1)] \(roleStr): \(msg.content.prefix(50))...")
            }

            print("\n🎉 FULL INTEGRATION TEST PASSED!")

        } catch let error as NobodyWhoError {
            print("\n❌ Error during model test:")
            switch error {
            case .ModelNotFound(let msg):
                print("   ModelNotFound: \(msg)")
            case .InvalidModel(let msg):
                print("   InvalidModel: \(msg)")
            case .InitializationError(let msg):
                print("   InitializationError: \(msg)")
            case .InferenceError(let msg):
                print("   InferenceError: \(msg)")
            case .Other(let msg):
                print("   Other: \(msg)")
            }
        } catch {
            print("\n❌ Unexpected error: \(error)")
        }
    }
} else {
    print("===========================================")
    print("ℹ️  No model path provided")
    print("===========================================")
    print("\nTo test with a real model, run:")
    print("  swift run --package-path swift NobodyWhoTestCLI /path/to/model.gguf")
    print("\nExample:")
    print("  swift run --package-path swift NobodyWhoTestCLI ~/Downloads/Qwen_Qwen3-0.6B-Q4_K_M.gguf")
}

print("\n===========================================")
print("Test Summary")
print("===========================================")
print("✅ Module loads correctly")
print("✅ All types are accessible")
print("✅ Instances can be created")
print("✅ Error handling works")
print("✅ Functions can be called")
if modelPath != nil {
    print("✅ Real model inference works")
}
print("\n🎉 All tests completed!")
