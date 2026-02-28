import XCTest
@testable import NobodyWho

final class NobodyWhoTests: XCTestCase {

    // MARK: - Type Existence Tests

    func testTypesExist() {
        // Verify all main types are accessible
        XCTAssertNotNil(ChatConfig.self)
        XCTAssertNotNil(Message.self)
        XCTAssertNotNil(Role.self)
        XCTAssertNotNil(Chat.self)
        XCTAssertNotNil(Model.self)
        XCTAssertNotNil(NobodyWhoError.self)
    }

    func testRoleEnum() {
        // Test Role enum cases
        let user = Role.user
        let assistant = Role.assistant
        let system = Role.system
        let tool = Role.tool

        XCTAssertNotNil(user)
        XCTAssertNotNil(assistant)
        XCTAssertNotNil(system)
        XCTAssertNotNil(tool)
    }

    func testChatConfigCreation() {
        // Test ChatConfig creation
        let config = ChatConfig(
            contextSize: 4096,
            systemPrompt: "You are a helpful assistant"
        )

        XCTAssertEqual(config.contextSize, 4096)
        XCTAssertEqual(config.systemPrompt, "You are a helpful assistant")
    }

    func testChatConfigWithNilPrompt() {
        // Test ChatConfig with nil system prompt
        let config = ChatConfig(
            contextSize: 2048,
            systemPrompt: nil
        )

        XCTAssertEqual(config.contextSize, 2048)
        XCTAssertNil(config.systemPrompt)
    }

    func testMessageCreation() {
        // Test Message creation
        let message = Message(
            role: Role.user,
            content: "Hello, world!"
        )

        XCTAssertEqual(message.role, Role.user)
        XCTAssertEqual(message.content, "Hello, world!")
    }

    // MARK: - Error Tests

    func testNobodyWhoErrorCases() {
        // Verify all error cases exist
        let errors: [NobodyWhoError] = [
            .ModelNotFound(message: "test"),
            .InvalidModel(message: "test"),
            .InitializationError(message: "test"),
            .InferenceError(message: "test"),
            .Other(message: "test")
        ]

        XCTAssertEqual(errors.count, 5)
    }

    func testNobodyWhoErrorMessages() {
        // Test error messages
        let modelNotFound = NobodyWhoError.ModelNotFound(message: "Model not found")
        let invalidModel = NobodyWhoError.InvalidModel(message: "Invalid model")

        // Verify they're different
        XCTAssertNotEqual(modelNotFound, invalidModel)
    }

    func testLoadModelWithInvalidPath() {
        // Test loading a non-existent model
        XCTAssertThrowsError(try loadModel(path: "/nonexistent/path/model.gguf", useGpu: false)) { error in
            guard let nobodyWhoError = error as? NobodyWhoError else {
                XCTFail("Expected NobodyWhoError")
                return
            }

            // Should be ModelNotFound or InvalidModel
            switch nobodyWhoError {
            case .ModelNotFound, .InvalidModel:
                break // Expected
            default:
                XCTFail("Expected ModelNotFound or InvalidModel, got \(nobodyWhoError)")
            }
        }
    }

    // MARK: - Function Tests

    func testInitLoggingDoesNotCrash() {
        // Test that initLogging can be called without crashing
        XCTAssertNoThrow(initLogging())
    }

    // MARK: - Equatable Tests

    func testMessageEquality() {
        let message1 = Message(role: .user, content: "Hello")
        let message2 = Message(role: .user, content: "Hello")
        let message3 = Message(role: .assistant, content: "Hello")
        let message4 = Message(role: .user, content: "Hi")

        XCTAssertEqual(message1, message2)
        XCTAssertNotEqual(message1, message3)
        XCTAssertNotEqual(message1, message4)
    }

    func testChatConfigEquality() {
        let config1 = ChatConfig(contextSize: 4096, systemPrompt: "Test")
        let config2 = ChatConfig(contextSize: 4096, systemPrompt: "Test")
        let config3 = ChatConfig(contextSize: 2048, systemPrompt: "Test")
        let config4 = ChatConfig(contextSize: 4096, systemPrompt: nil)

        XCTAssertEqual(config1, config2)
        XCTAssertNotEqual(config1, config3)
        XCTAssertNotEqual(config1, config4)
    }

    // MARK: - Hashable Tests

    func testMessageHashable() {
        let message1 = Message(role: .user, content: "Hello")
        let message2 = Message(role: .user, content: "Hello")

        var set = Set<Message>()
        set.insert(message1)
        set.insert(message2)

        // Should only have 1 element since they're equal
        XCTAssertEqual(set.count, 1)
    }

    func testChatConfigHashable() {
        let config1 = ChatConfig(contextSize: 4096, systemPrompt: "Test")
        let config2 = ChatConfig(contextSize: 4096, systemPrompt: "Test")

        var set = Set<ChatConfig>()
        set.insert(config1)
        set.insert(config2)

        // Should only have 1 element since they're equal
        XCTAssertEqual(set.count, 1)
    }
}
