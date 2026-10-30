import XCTest
@testable import NobodyWho

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
        for try await token in try await chat.ask("Say hi") {
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

    // MARK: - Complete

    func testComplete() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let noThinking = ["enable_thinking": false]
        let chat = try Chat(model: model, systemPrompt: "Reply with one word only.", templateVariables: noThinking)

        let messages: [Message] = [
            .user("Who was the first person to walk on the moon?"),
            .assistant("Neil Armstrong."),
            .user("Which year did he do it? Answer with only the year."),
        ]
        let response = try await chat.complete(messages).completed()
        XCTAssertTrue(response.contains("1969"), "Model did not read the supplied history: \(response)")

        // The supplied messages replace the history, with the reply appended
        let history = await chat.getChatHistory()
        XCTAssertEqual(history.count, messages.count + 1)

        // An invalid conversation is rejected at the call site
        do {
            _ = try await chat.complete([
                .user("Hi"),
                .assistant("Aye, "),
            ])
            XCTFail("Expected a trailing assistant message to be rejected")
        } catch {
            // expected
        }

        // Options stick: what they set stays set, what they omit is kept
        _ = try await chat.complete(
            [.user("Say hi.")],
            options: Options(templateVariables: ["enable_thinking": true])
        ).completed()
        let after = await chat.getTemplateVariables()
        XCTAssertEqual(after, ["enable_thinking": true])

        _ = try await chat.complete([.user("Say hi again.")]).completed()
        let unchanged = await chat.getTemplateVariables()
        XCTAssertEqual(
            unchanged, ["enable_thinking": true],
            "An empty Options should leave the template variables alone")
    }

    // MARK: - Tokenize

    func testTokenize() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let chat = try Chat(model: model, templateVariables: ["enable_thinking": false])
        let tokens = try await chat.tokenize(message: "Hey!")
        XCTAssertEqual(tokens, [18665, 0])
    }

    // MARK: - Encoder

    func testEncoderBatch() async throws {
        let modelPath = try requireEnv("TEST_EMBEDDINGS_MODEL")
        let model = try await Model.load(modelPath: modelPath, useGpu: false)
        let encoder = Encoder(model: model, contextSize: 1024)
        let texts = ["Copenhagen is in Denmark.", "Berlin is in Germany."]
        var individual: [[Float]] = []
        for text in texts {
            individual.append(try await encoder.encode(text))
        }
        let batched = try await encoder.encodeBatch(texts)

        XCTAssertEqual(batched.count, texts.count)
        for (expected, actual) in zip(individual, batched) {
            XCTAssertEqual(expected.count, actual.count)
            for (expectedValue, actualValue) in zip(expected, actual) {
                XCTAssertEqual(expectedValue, actualValue, accuracy: 1e-5)
            }
        }
    }

    // MARK: - Stats

    func testStats() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        let chat = try Chat(model: model, templateVariables: ["enable_thinking": false])
        try await chat.ask("What is the capital of Denmark?").completed()
        let stats = await chat.getStats()
        XCTAssertGreaterThan(stats.contextUsed, 0)
        XCTAssertLessThanOrEqual(stats.contextUsed, stats.contextSize)
    }

    func testContextShiftOptions() async throws {
        let modelPath = try requireEnv("TEST_MODEL")
        let model = try await Model.load(modelPath: modelPath)
        _ = try Chat(
            model: model,
            contextSize: 1024,
            contextShift: ContextShiftOptions(keepFirstTurns: 2, keepLastTurns: 3, target: .tokens(tokens: 256))
        )
        _ = try Chat(model: model, contextShift: ContextShiftOptions(enabled: false))
        XCTAssertThrowsError(
            try Chat(model: model, contextSize: 1024, contextShift: ContextShiftOptions(target: .tokens(tokens: 1024)))
        )

        let chat = try Chat(model: model)
        try await chat.setContextShift(ContextShiftOptions(target: .fraction(fraction: 0.25)))
        do {
            try await chat.setContextShift(ContextShiftOptions(keepLastTurns: 0))
            XCTFail("keepLastTurns = 0 should be rejected")
        } catch {}
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

    // MARK: - SpeechToText (Whisper)

    func testSpeechToText() async throws {
        // Model: HuggingFace repo (hf://owner/repo) or local dir. Downloaded and cached on first run.
        let model = ProcessInfo.processInfo.environment["TEST_WHISPER_MODEL"]
            ?? "hf://onnx-community/whisper-base"

        // Audio: "Hey Ron. Hey Billy." — shared asset in assets/.
        // TEST_AUDIO_FILE is set in CI to an absolute path; fall back to the
        // relative path for local runs from nobodywho/swift/.
        let audioFile = ProcessInfo.processInfo.environment["TEST_AUDIO_FILE"]
            ?? "../../assets/sound.mp3"

        // Use fp32 ("default"): the q4 whisper-base encoder mis-transcribes
        // "Billy" as "Bailey", while fp32 gets it right.
        let stt = try await SpeechToText.load(source: model, quantization: "default")
        let text = try await stt.transcribeFile(path: audioFile).completed()
        XCTAssertTrue(text.lowercased().contains("ron"))
        XCTAssertTrue(text.lowercased().contains("billy"))
    }

    // MARK: - Decisions

    private static let referenceState =
        "Customer message: I was charged twice for my order last week and nobody has replied."

    private static let referenceQuestions: OrderedFields<DecisionQuestion> = [
        "route": .choice(
            instructions: .string("Choose the best team for this customer message."),
            criteria: [
                "billing": .string("payments, charges, refunds, invoices"),
                "shipping": .string("delivery, tracking, lost or late parcels"),
                "technical": .string("bugs, errors, login problems"),
            ]
        ),
        "angry": .noul(instructions: .string("Is the customer angry?")),
        "urgency": .score(
            instructions: .string("How urgent is this?"),
            criteria: [.string("can wait"), .string("this week"), .string("today"), .string("right now")]
        ),
    ]

    func testDecision() async throws {
        let modelPath = try requireEnv("TEST_DECISION_MODEL")
        let model = try await Model.load(modelPath: modelPath, useGpu: false)
        let decision = try Decision(model: model, contextSize: 1024)
        let response = try await decision.decide(
            state: .string(Self.referenceState),
            questions: Self.referenceQuestions
        )

        XCTAssertEqual(Set(response.answers.keys), Set(Self.referenceQuestions.entries.map(\.0)))

        guard let route = response.answers["route"],
              case let .choice(choice, probabilities, confidence) = route else {
            return XCTFail("route answer is not a choice")
        }
        XCTAssertNotNil(probabilities[choice])
        XCTAssertEqual(probabilities.values.reduce(0, +), 1.0, accuracy: 0.01)
        XCTAssertTrue((0.0...1.0).contains(confidence))

        guard let angry = response.answers["angry"],
              case let .noul(noul) = angry else {
            return XCTFail("angry answer is not a noul")
        }
        XCTAssertTrue((0.0...1.0).contains(noul))

        guard let urgency = response.answers["urgency"],
              case let .score(score, legend, _, _) = urgency else {
            return XCTFail("urgency answer is not a score")
        }
        XCTAssertTrue((0.0...Double(legend.count - 1)).contains(score))

        XCTAssertGreaterThan(response.usage.inputTokens, 0)
        XCTAssertGreaterThanOrEqual(response.usage.outputTokens, 0)
    }

    func testDecisionJsonKeepsInputOrder() throws {
        let questions: OrderedFields<DecisionQuestion> = [
            "z-route": .choice(
                instructions: .object(["z-task": .string("route"), "a-context": .string("support")]),
                criteria: ["technical": .string("bugs"), "billing": .string("charges")]
            ),
            "a-refund": .noul(instructions: .string("Refund?"), criteria: ["true": .string("yes"), "false": .string("no")]),
        ]
        XCTAssertEqual(
            try decisionQuestionsString(questions: questions),
            #"{"z-route":{"type":"choice","instructions":{"z-task":"route","a-context":"support"},"criteria":{"technical":"bugs","billing":"charges"}},"a-refund":{"type":"noul","instructions":"Refund?","criteria":{"true":"yes","false":"no"}}}"#
        )
    }

    func testDecisionRejectsEmptyQuestions() async throws {
        let modelPath = try requireEnv("TEST_DECISION_MODEL")
        let model = try await Model.load(modelPath: modelPath, useGpu: false)
        let decision = try Decision(model: model, contextSize: 1024)
        do {
            _ = try await decision.decide(state: .string(Self.referenceState), questions: [:])
            XCTFail("empty questions were accepted")
        } catch {
            // expected
        }
    }
}
