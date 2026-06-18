import XCTest
import NobodyWho

/// Integration tests for the NobodyWho Swift bindings.
///
/// Requires the `NobodyWhoNative` xcframework built locally
/// (`swift/scripts/build-swift-xcframework.sh`).
///
/// Environment variables (matching nobodywho/models.nix):
/// - `TEST_MODEL` — path to a chat GGUF model (required)
/// - `TEST_VISION_MODEL` — path to a vision GGUF model (optional, for vision test)
/// - `TEST_MMPROJ` — path to a multimodal projector GGUF (optional, for vision test)
///
/// Run: TEST_MODEL=/path/to/model.gguf swift test --filter NobodyWhoTests

// Top-level tool declaration using the macro (peer macros don't work inside function bodies)
@DeclareTool("Echo back the input")
func echo(message: String) -> String {
    return message
}

final class NobodyWhoTests: XCTestCase {

    private func requireEnv(_ name: String) throws -> String {
        guard let value = ProcessInfo.processInfo.environment[name] else {
            throw XCTSkip("\(name) environment variable not set")
        }
        return value
    }

    // MARK: - Download

    func testDownloadModel() async throws {
        let modelUrl = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"

        // First download to discover the cache path, then delete to force a fresh download
        let cachedPath = try await Model.downloadModel(modelPath: modelUrl)
        try FileManager.default.removeItem(atPath: cachedPath)

        var progressCalled = false
        let localPath = try await Model.downloadModel(modelPath: modelUrl) { downloaded, total in
            progressCalled = true
            XCTAssertLessThanOrEqual(downloaded, total)
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: localPath))
        let attrs = try FileManager.default.attributesOfItem(atPath: localPath)
        XCTAssertGreaterThan(attrs[.size] as? UInt64 ?? 0, 0)
        XCTAssertTrue(progressCalled, "Progress callback should have been called")
    }

    // MARK: - Cached models

    func testGetCachedModels() throws {
        // Just verify the FFI bridge works — the cache may be empty on CI.
        let models = try getCachedModels()
        for m in models {
            XCTAssertFalse(m.path.isEmpty)
            XCTAssertGreaterThan(m.size, 0)
        }
    }

    // MARK: - Chat (completion, streaming, tools)

    func testChat() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let noThinking = ["enable_thinking": false]
        let chat = try Chat(model: model, systemPrompt: "Reply with one word only.", templateVariables: noThinking)

        // Completion
        let response = try await chat.ask("Say hello").completed()
        XCTAssertFalse(response.isEmpty)

        // Streaming
        try await chat.resetContext(systemPrompt: "Reply briefly.")
        var tokens: [String] = []
        for try await token in chat.ask("Say hi") {
            tokens.append(token)
        }
        XCTAssertFalse(tokens.isEmpty)

        // Macro-generated tool
        try await chat.resetContext(systemPrompt: "Use the echo tool to echo back exactly what the user says.", tools: [echoTool])
        let echoResponse = try await chat.ask("Echo: hello").completed()
        XCTAssertFalse(echoResponse.isEmpty)

        // Manual tool with callback verification
        var called = false
        let pingTool = Tool(
            name: "ping",
            description: "Ping the server",
            parameters: []
        ) { _ in
            called = true
            return "pong"
        }
        try await chat.resetContext(systemPrompt: "Use the ping tool now.", tools: [pingTool])
        let _ = try await chat.ask("Ping the server").completed()
        XCTAssertTrue(called)
    }

    // MARK: - Tokenize

    func testTokenize() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let chat = try Chat(model: model, templateVariables: ["enable_thinking": false])
        let tokens = try await chat.tokenize(message: "Hey!")
        XCTAssertEqual(tokens, [18665, 0])
    }

    // MARK: - Stats

    func testStats() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let chat = try Chat(model: model, templateVariables: ["enable_thinking": false])
        try await chat.ask("What is the capital of Denmark?").completed()
        let stats = try await chat.getStats()
        XCTAssertGreaterThan(stats.contextUsed, 0)
        XCTAssertLessThanOrEqual(stats.contextUsed, stats.contextSize)
    }

    // MARK: - Vision

    func testVision() async throws {
        let modelPath = try requireEnv("TEST_VISION_MODEL")
        let mmprojPath = try requireEnv("TEST_MMPROJ")

        // Use the test image from the python tests
        let imagePath = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // tests/NobodyWhoTests
            .deletingLastPathComponent() // tests
            .deletingLastPathComponent() // swift
            .appendingPathComponent("../python/tests/img/dog.png")
            .standardized.path

        let model = try await Model.load(modelPath: modelPath, projectionModelPath: mmprojPath)
        let chat = try Chat(model: model, systemPrompt: "Describe what you see briefly.")

        let prompt = Prompt([
            Prompt.image(imagePath),
            Prompt.text("What is in this image?"),
        ])
        let response = try await chat.ask(prompt).completed()
        XCTAssertFalse(response.isEmpty)
    }

    // MARK: - Translator

    func testTranslator() async throws {
        let modelPath = try requireEnv("TEST_TRANSLATE_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let translator = try Translator(model: model, source: "en", target: "da")
        let result = try await translator.translate("Hello, how are you?").completed()
        XCTAssertTrue(result.contains("Hej"), "Expected 'Hej' in translation, got: \(result)")
    }
}
