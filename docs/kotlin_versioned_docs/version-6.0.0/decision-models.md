---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
sidebar_position: 6
---

A decision model scores predefined answers instead of generating text. When you need to classify data and already know the possible answers, these models are a good choice.

## Load a model

NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models.

```kotlin
import ai.nobodywho.Decision
import ai.nobodywho.DecisionQuestion
import ai.nobodywho.Model

suspend fun main() {
    val model = Model.load(
        modelPath = "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
        useGpu = true
    )
    val decision = Decision(model = model, contextSize = 1024u)
    // Or load the decision model directly:
    // val decision = Decision.fromPath(modelPath = "./Laya-Q8_0.gguf", useGpu = true)
}
```

## Make a decision

Call `decide` with a state and a map of question IDs to questions. State and instructions can be text or structured data.

```kotlin
suspend fun main() {
    val decision = Decision.fromPath(
        modelPath = "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
        useGpu = true,
        contextSize = 1024u
    )
    val response = decision.decide(
        state = mapOf("message" to "I was charged twice. Please refund the duplicate payment."),
        questions = mapOf(
            "department" to DecisionQuestion.Choice(
                instructions = "Which team should handle this?",
                criteria = mapOf("billing" to "Payments and refunds", "technical" to "Bugs and login problems")
            ),
            "refund" to DecisionQuestion.Noul(instructions = "Is a refund requested?"),
            "urgency" to DecisionQuestion.Score(
                instructions = "How urgent is this?",
                criteria = listOf("can wait", "soon", "urgent")
            )
        )
    )
    println(response.answers["department"])
    println(response.usage.outputTokens) // Always 0
}
```

### Select an option with `Choice`

Use `Choice` when one of a set of named options is the answer. Its criteria map option IDs to descriptions. The answer includes the selected ID and probabilities for each option.

### Check a condition with `Noul`

Use `Noul` for a yes-or-no question. Its `noul` value is the probability that the answer is yes, from 0 to 1. Criteria are optional.

### Rate content with `Score`

Use `Score` for an ordered scale. Provide two to ten descriptions in `criteria`, from lowest to highest. The score is the probability-weighted average of zero-based levels, not a probability. Its legend maps level IDs to descriptions.

## Read the return values

`decide` returns a `DecisionResponse` with `answers` and `usage`.

- `answers` maps question IDs to `DecisionAnswer` values. `Choice` includes `choice`, `probabilities`, and `confidence`; `Score` includes `score`, `legend`, `probabilities`, and `confidence`; `Noul` includes `noul`.
- `confidence` indicates how concentrated the probabilities are. It is available for choices and scores, and does not guarantee correctness.
- `usage` contains `inputTokens` and `outputTokens`. Decisions run non-generatively, so `outputTokens` is always `0`.

## Configure GPU use

GPU use is enabled by default when loading a model. Set `useGpu = false` in `Model.load` or `Decision.fromPath` to disable it.
