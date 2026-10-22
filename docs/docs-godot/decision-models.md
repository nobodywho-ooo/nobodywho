---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
sidebar_position: 6
---

A decision model scores predefined answers instead of generating text. When you need to classify data and already know the possible answers, these models are a good choice.

## Load a model

NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models.

Create a decision model directly from its path, or pass a `NobodyWhoModel`. **Await the create call immediately.**

```gdscript
var decision = await NobodyWhoDecision.create(
    "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
    {"n_ctx": 1024}
)
if decision == null:
    push_error("Could not load decision model")
    return
```

You can also create and pass a shared model:

```gdscript
var model = await NobodyWhoModel.create("res://model.gguf", {})
var decision = await NobodyWhoDecision.create(model, {"n_ctx": 1024})
```

## Make a decision

Call `decide` with a state and a dictionary of question IDs. State can be a `String`, `Dictionary`, or `Array`.

```gdscript
var state = {"message": "I was charged twice. Please refund the duplicate payment."}
var questions = {
    "department": {
        "type": "choice",
        "instructions": "Which team should handle this?",
        "criteria": {"billing": "Payments and refunds", "technical": "Bugs and login problems"},
    },
    "refund": {
        "type": "noul",
        "instructions": "Is a refund requested?",
    },
    "urgency": {
        "type": "score",
        "instructions": "How urgent is this?",
        "criteria": ["can wait", "soon", "urgent"],
    },
}
var response = await decision.decide(state, questions)
if response == null:
    push_error("Decision failed")
    return
print(response["answers"]["department"]["choice"])
print(response["usage"]["output_tokens"]) # Always 0
```

### Select an option with `choice`

Use `choice` when one of a set of named options is the answer. Its criteria dictionary maps option IDs to descriptions. The answer includes the selected ID and probabilities for each option.

### Check a condition with `noul`

Use `noul` for a yes-or-no question. Its `noul` value is the probability that the answer is yes, from 0 to 1. Criteria are optional.

### Rate content with `score`

Use `score` for an ordered scale. Provide two to ten descriptions in `criteria`, from lowest to highest. The score is the probability-weighted average of zero-based levels, not a probability. Its legend maps level IDs to descriptions.

## Read the return values

`decide` returns a dictionary with `answers` and `usage`, or `null` on failure.

- `answers` maps question IDs to answer dictionaries. Each has a `type`. `choice` includes `choice`, `probabilities`, and `confidence`; `score` includes `score`, `legend`, `probabilities`, and `confidence`; `noul` includes `noul`.
- `confidence` indicates how concentrated the probabilities are. It is available for choices and scores, and does not guarantee correctness.
- `usage` contains `input_tokens` and `output_tokens`. Decisions run non-generatively, so `output_tokens` is always `0`.
