---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
sidebar_position: 6
---

A decision model scores predefined answers instead of generating text. NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models.

## Load a model

```typescript
import { Decision } from "react-native-nobodywho";

const decision = await Decision.fromPath({
  modelPath: "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
});
```

You can also create a decision model from a loaded model:

```typescript
import { Decision, Model } from "react-native-nobodywho";

const model = await Model.load({ modelPath: "/path/to/model.gguf" });
const decision = new Decision({ model, contextSize: 1024 });
```

## Make a decision

Pass text or any JSON value as `state`. `questions` is a record keyed by question ID. Each question has instructions and criteria appropriate to its type.

```typescript
import { Decision } from "react-native-nobodywho";

async function classify() {
  const decision = await Decision.fromPath({
    modelPath: "hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf",
  });
  const response = await decision.decide({
    state: { message: "I was charged twice. Please refund the duplicate payment." },
    questions: {
      department: {
        type: "choice",
        instructions: "Which team should handle this?",
        criteria: { billing: "Payments and refunds", technical: "Bugs and login problems" },
      },
      refund: { type: "noul", instructions: "Is a refund requested?" },
      urgency: {
        type: "score",
        instructions: "How urgent is this?",
        criteria: ["can wait", "soon", "urgent"],
      },
    },
  });

  console.log(response.answers.department);
  console.log(response.answers.refund);
  console.log(response.answers.urgency);
  console.log(response.usage.input_tokens, response.usage.output_tokens); // output_tokens is always 0
  decision.destroy();
}

void classify();
```

## Choose a question type

### Select an option with `choice`

Provide a record of option IDs and descriptions in `criteria`. The answer's `choice` is the selected ID; `probabilities` gives the probability for each option.

### Check a condition with `noul`

Use `noul` for a yes-or-no judgment. Its `noul` value is the probability that the answer is yes, from 0 to 1. Optional criteria can provide additional labels or descriptions.

### Rate content with `score`

Provide two to ten ordered descriptions in `criteria`, from lowest to highest. The score is a probability-weighted average of the zero-based levels, not a probability. `legend` maps level IDs to descriptions, and `probabilities` gives the probability for each level.

## Read the return values

`decide` returns a `DecisionResponse` with `answers` and `usage`. Each answer is a `DecisionAnswer` matching its question type. `choice` and `score` answers include `confidence`, a value from 0 to 1. `score` also includes `legend`; `noul` contains only its probability. High confidence does not guarantee a correct answer.

`usage.input_tokens` counts input tokens evaluated across questions. `usage.output_tokens` is always `0` because decision models do not generate text. Call `decision.destroy()` when finished to free native resources.
