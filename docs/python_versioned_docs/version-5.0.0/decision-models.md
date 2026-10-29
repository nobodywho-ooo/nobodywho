---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
---

A decision model scores predefined answers instead of generating text. [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev) popularized this System One approach. When you need to classify text and already know the possible answers, these models are a good choice!

## Load a model

NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models.

```python
from nobodywho import Decision, Model

model = Model(model_path="hf://ggml-org/Laya-GGUF/Laya-Q8_0.gguf")
decisions = Decision(model=model, n_ctx=1024)
```

To load a local model, pass its GGUF path as `model_path`.

## Make a decision

Call `decide` with two keyword-only arguments:

- `state`: the text or JSON-compatible data to evaluate.
- `questions`: a `dict` of question IDs. Each question has a `type`, `instructions`, and any `criteria` the type requires.

```python continuation
message = "I was charged twice. Please refund the duplicate payment."
questions = {
	"department": {
		"type": "choice",
		"instructions": "Which team should handle this?",
		"criteria": {
			"billing": "Payments and refunds",
			"technical": "Bugs and login problems",
		},
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

result = decisions.decide(state={"message": message}, questions=questions)
team = result["answers"]["department"]["choice"]
probabilities = result["answers"]["department"]["probabilities"]
refund_probability = result["answers"]["refund"]["noul"]
urgency = result["answers"]["urgency"]["score"]
assert result["usage"]["output_tokens"] == 0
```

## Choose a question type

### Select an option with `choice`

To route an email to a team, use `choice`. A `choice` value of `"billing"` selects that team. Read `probabilities` for the probability of each option.

### Check a condition with `noul`

To check whether the sender requests a refund, use `noul`. A `noul` value of `0.9` means the model estimates a 90% probability that the answer is yes.

### Rate content with `score`

Use `score` when the answers form an ordered scale. Define two to ten descriptive levels in `criteria`, from lowest to highest.

Consider the urgency example above. The levels are `can wait` at 0, `soon` at 1, and `urgent` at 2. A score of `1.6` is between `soon` and `urgent`.

The score is a weighted average, not a probability. To find the probability of `urgent`, read `probabilities["2"]`. The `legend` maps level IDs to their descriptions.

## Read the return values

`decide` returns a Python `dict` with `answers` and `usage`. Question IDs and option IDs keep their insertion order.

- `answers`: an answer `dict` for each question ID. Each answer contains the fields for its question type:
  - `type`: `choice`, `noul`, or `score`.
  - `choice`: the selected option ID.
  - `noul`: the probability that the answer is yes, from 0 to 1.
  - `score`: the probability-weighted average of the zero-based levels.
  - `probabilities`: the probability of each option or level, for `choice` and `score`.
  - `confidence`: how concentrated the probabilities are, from 0 to 1. Only `choice` and `score` return `confidence`. High confidence does not guarantee a correct answer.
  - `legend`: level IDs mapped to their descriptions, for `score`.
- `usage`: token counts:
  - `input_tokens`: the number of input tokens evaluated across all questions.
  - `output_tokens`: always `0`. Decision models do not generate text.

## Use async inference

To await inference, use `DecisionAsync`. The constructor is synchronous.

```python continuation
import asyncio
from nobodywho import DecisionAsync

async_decisions = DecisionAsync(model=model, n_ctx=1024)

async def classify():
	return await async_decisions.decide(state={"message": message}, questions=questions)

async_result = asyncio.run(main=classify())
assert async_result["usage"]["output_tokens"] == 0
```

## Configure GPU use

Use the [same model and GPU settings as chat](./chat#gpu).
