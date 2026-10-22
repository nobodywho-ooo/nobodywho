import Foundation
import NobodyWhoGenerated

/// Ordered fields for decision inputs.
public struct OrderedFields<Value>: ExpressibleByDictionaryLiteral {
    public let entries: [(String, Value)]

    public init(entries: [(String, Value)]) {
        self.entries = entries
    }

    public init(dictionaryLiteral elements: (String, Value)...) {
        self.entries = elements
    }
}

/// A JSON value, used for decision states, instructions, criteria, and legends.
public indirect enum JSONValue {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object(OrderedFields<JSONValue>)
    case array([JSONValue])
    case null

    /// Wrap a value produced by `JSONSerialization`. Dictionary order cannot be recovered.
    /// For ordered inputs, construct `.object` directly instead.
    public init(rawValue: Any) {
        switch rawValue {
        case let value as String: self = .string(value)
        case let value as NSNumber:
            // `NSNumber` also covers booleans, so check the underlying type.
            if String(cString: value.objCType) == "c" {
                self = .bool(value.boolValue)
            } else {
                self = .number(value.doubleValue)
            }
        case let value as [String: Any]:
            self = .object(OrderedFields(entries: value.map { ($0.key, JSONValue(rawValue: $0.value)) }))
        case let value as [Any]: self = .array(value.map(JSONValue.init(rawValue:)))
        default: self = .null
        }
    }

    /// The value as `JSONSerialization` understands it. Object ordering is lost.
    public var rawValue: Any {
        switch self {
        case let .string(value): return value
        case let .number(value): return value
        case let .bool(value): return value
        case let .object(value):
            return value.entries.reduce(into: [String: Any]()) { result, entry in
                result[entry.0] = entry.1.rawValue
            }
        case let .array(value): return value.map(\.rawValue)
        case .null: return NSNull()
        }
    }
}

/// A question asked of a decision model. Matches the Laya question types: a
/// single choice, an ordered score, or a boolean (`noul`).
public enum DecisionQuestion {
    /// Pick one option. `criteria` lists option ids and descriptions in input order.
    case choice(instructions: JSONValue, criteria: OrderedFields<JSONValue>)
    /// Score from lowest to highest. `criteria` holds 2 to 10 ordered descriptions.
    case score(instructions: JSONValue, criteria: [JSONValue])
    /// A boolean question. `criteria` optionally describes the "false" and "true" answers.
    case noul(instructions: JSONValue, criteria: OrderedFields<JSONValue>? = nil)

    func json() -> JSONValue {
        switch self {
        case let .choice(instructions, criteria):
            return .object([
                "type": .string("choice"),
                "instructions": instructions,
                "criteria": .object(criteria),
            ])
        case let .score(instructions, criteria):
            return .object([
                "type": .string("score"),
                "instructions": instructions,
                "criteria": .array(criteria),
            ])
        case let .noul(instructions, criteria):
            var fields: [(String, JSONValue)] = [
                ("type", .string("noul")),
                ("instructions", instructions),
            ]
            if let criteria {
                fields.append(("criteria", .object(criteria)))
            }
            return .object(OrderedFields(entries: fields))
        }
    }
}

/// One answer to a ``DecisionQuestion``, with the probabilities behind it.
public enum DecisionAnswer {
    /// Picked option id, its probabilities, and separation from a uniform distribution.
    case choice(choice: String, probabilities: [String: Double], confidence: Double)
    /// Expected zero-based level index (can fall between levels).
    case score(score: Double, legend: [String: JSONValue], probabilities: [String: Double], confidence: Double)
    /// Probability that the answer is "true".
    case noul(noul: Double)
}

/// Token usage for one ``Decision/decide(state:questions:)`` call.
public struct DecisionUsage: Equatable {
    public let inputTokens: Int
    public let outputTokens: Int
}

/// Answers keyed by question name, plus the token usage of the run.
public struct DecisionResponse {
    public let answers: [String: DecisionAnswer]
    public let usage: DecisionUsage
}

/// Typed, non-generative decisions over text or JSON with a Laya GGUF model.
/// Every answer comes with probabilities instead of generated text.
///
/// ```swift
/// let decision = try await Decision.fromPath(modelPath: "Laya-Q8_0.gguf")
/// let response = try await decision.decide(
///     state: .string("I was charged twice. Please refund."),
///     questions: ["refund": .noul(instructions: .string("Is a refund requested?"))]
/// )
/// ```
public class Decision {
    private let inner: NobodyWhoGenerated.RustDecision

    /// - Parameters:
    ///   - model: A loaded Laya decision model.
    ///   - contextSize: Maximum context size in tokens. Defaults to 1024 if nil.
    public init(model: Model, contextSize: UInt32? = nil) throws {
        self.inner = try NobodyWhoGenerated.RustDecision(model: model.inner, nCtx: contextSize)
    }

    /// Create a decision handle directly from a model path.
    public static func fromPath(
        modelPath: String,
        useGpu: Bool = true,
        contextSize: UInt32? = nil
    ) async throws -> Decision {
        let model = try await Model.load(modelPath: modelPath, useGpu: useGpu)
        return try Decision(model: model, contextSize: contextSize)
    }

    /// Run the questions over `state`, which may be text, an object, or an array.
    public func decide(
        state: JSONValue,
        questions: OrderedFields<DecisionQuestion>
    ) async throws -> DecisionResponse {
        let questionsJson = try decisionQuestionsString(questions: questions)
        let responseJson = try await inner.decide(
            stateJson: try state.jsonString(),
            questionsJson: questionsJson
        )
        return try decisionResponse(from: responseJson)
    }
}

func decisionQuestionsString(questions: OrderedFields<DecisionQuestion>) throws -> String {
    try JSONValue.object(OrderedFields(entries: questions.entries.map { ($0.0, $0.1.json()) })).jsonString()
}

extension JSONValue {
    func jsonString() throws -> String {
        switch self {
        case let .object(fields):
            let entries = try fields.entries.map { key, value in
                try JSONValue.string(key).jsonString() + ":" + value.jsonString()
            }
            return "{" + entries.joined(separator: ",") + "}"
        case let .array(values):
            return "[" + (try values.map { try $0.jsonString() }).joined(separator: ",") + "]"
        default:
            let data = try JSONSerialization.data(withJSONObject: rawValue, options: [.fragmentsAllowed])
            return String(decoding: data, as: UTF8.self)
        }
    }
}

private func decisionResponse(from json: String) throws -> DecisionResponse {
    guard let data = json.data(using: .utf8),
          let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
          let answers = root["answers"] as? [String: [String: Any]],
          let usage = root["usage"] as? [String: Any] else {
        throw DecisionDecodingError.malformedResponse
    }
    return DecisionResponse(
        answers: try answers.mapValues(decisionAnswer(from:)),
        usage: DecisionUsage(
            inputTokens: usage["input_tokens"] as? Int ?? 0,
            outputTokens: usage["output_tokens"] as? Int ?? 0
        )
    )
}

private func decisionAnswer(from json: [String: Any]) throws -> DecisionAnswer {
    switch json["type"] as? String {
    case "choice":
        return .choice(
            choice: json["choice"] as? String ?? "",
            probabilities: json["probabilities"] as? [String: Double] ?? [:],
            confidence: json["confidence"] as? Double ?? 0
        )
    case "score":
        let legend = (json["legend"] as? [String: Any] ?? [:]).mapValues(JSONValue.init(rawValue:))
        return .score(
            score: json["score"] as? Double ?? 0,
            legend: legend,
            probabilities: json["probabilities"] as? [String: Double] ?? [:],
            confidence: json["confidence"] as? Double ?? 0
        )
    case "noul":
        return .noul(noul: json["noul"] as? Double ?? 0)
    default:
        throw DecisionDecodingError.malformedResponse
    }
}

public enum DecisionDecodingError: Error {
    case malformedResponse
}
