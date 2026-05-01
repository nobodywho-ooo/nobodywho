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

    // MARK: - Completion

    func testCompletion() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let chat = try await Chat.fromPath(modelPath: modelPath, systemPrompt: "Reply with one word only.")
        let response = try await chat.ask("Say hello").completed()
        XCTAssertFalse(response.isEmpty)
    }

    func testStreaming() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let chat = try await Chat.fromPath(modelPath: modelPath, systemPrompt: "Reply briefly.")
        var tokens: [String] = []
        for await token in chat.ask("Say hello") {
            tokens.append(token)
        }
        XCTAssertFalse(tokens.isEmpty)
    }

    // MARK: - Tool calling

    func testMacroTool() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)

        let chat = Chat(model: model, systemPrompt: "Use the echo tool to echo back exactly what the user says.", tools: [echoTool])
        let response = try await chat.ask("Echo: hello").completed()
        XCTAssertFalse(response.isEmpty)
    }

    func testSyncTool() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)

        var called = false

        let pingTool = Tool(
            name: "ping",
            description: "Ping the server",
            parameters: []
        ) { _ in
            called = true
            return "pong"
        }

        let chat = Chat(model: model, systemPrompt: "Use the ping tool now.", tools: [pingTool])
        let _ = try await chat.ask("Ping the server").completed()
        XCTAssertTrue(called)
    }

    func testAsyncTool() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)

        var called = false

        let getTimeTool = Tool(
            name: "getTime",
            description: "Get the current time",
            parameters: [("timezone", #"{"type": "string"}"#)]
        ) { args in
            called = true
            let timezone = args[0] as! String
            return "{\"time\": \"12:00\", \"timezone\": \"\(timezone)\"}"
        }

        let chat = Chat(model: model, systemPrompt: "Use the getTime tool now.", tools: [getTimeTool])
        let _ = try await chat.ask("What time is it in UTC?").completed()
        XCTAssertTrue(called)
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
        let chat = Chat(model: model, systemPrompt: "Describe what you see briefly.")

        let prompt = Prompt([
            Prompt.image(imagePath),
            Prompt.text("What is in this image?"),
        ])
        let response = try await chat.ask(prompt).completed()
        XCTAssertFalse(response.isEmpty)
    }
}
