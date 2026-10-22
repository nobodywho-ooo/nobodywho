---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
sidebar_position: 6
---

A decision model scores predefined answers instead of generating text. NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models.

## Load a model

```swift
import NobodyWho

let decision = try await Decision.fromPath(
    modelPath: "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf"
)
```

You can also create a decision model from a loaded model:

```swift
let model = try await Model.load(modelPath: "/path/to/model.gguf")
let decision = try Decision(model: model, contextSize: 1024)
```

## Make a decision

Pass a `JSONValue` as `state`. `questions` is a dictionary keyed by question ID. Each question has instructions and criteria appropriate to its type.

```swift
import NobodyWho

func classify() async throws {
    let decision = try await Decision.fromPath(
        modelPath: "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf"
    )
    let response = try await decision.decide(
        state: .object(["message": .string("I was charged twice. Please refund the duplicate payment.")]),
        questions: [
            "department": .choice(
                instructions: .string("Which team should handle this?"),
                criteria: ["billing": .string("Payments and refunds"), "technical": .string("Bugs and login problems")]
            ),
            "refund": .noul(instructions: .string("Is a refund requested?")),
            "urgency": .score(
                instructions: .string("How urgent is this?"),
                criteria: [.string("can wait"), .string("soon"), .string("urgent")]
            ),
        ]
    )

    guard case let .noul(probability) = response.answers["refund"] else { return }
    print(probability)
    print(response.answers["department"]!)
    print(response.answers["urgency"]!)
    print(response.usage.inputTokens, response.usage.outputTokens) // outputTokens is always 0
}
```

## Choose a question type

### Select an option with `choice`

Provide a dictionary of option IDs and descriptions in `criteria`. The answer's `choice` is the selected ID; `probabilities` gives the probability for each option.

### Check a condition with `noul`

Use `noul` for a yes-or-no judgment. Its value is the probability that the answer is yes, from 0 to 1. Optional criteria can provide additional labels or descriptions.

### Rate content with `score`

Provide two to ten ordered descriptions in `criteria`, from lowest to highest. The score is a probability-weighted average of the zero-based levels, not a probability. `legend` maps level IDs to descriptions, and `probabilities` gives the probability for each level.

## Read the return values

`decide` returns a `DecisionResponse` with `answers` and `usage`. Each answer is a `DecisionAnswer` matching its question type. `choice` and `score` answers include `confidence`, a value from 0 to 1. `score` also includes `legend`; `noul` contains only its probability. High confidence does not guarantee a correct answer.

`usage.inputTokens` counts input tokens evaluated across questions. `usage.outputTokens` is always `0` because decision models do not generate text.
