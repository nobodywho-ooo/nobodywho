---
title: Decision models
description: Make typed choices, Boolean judgments, and ordinal scores locally.
sidebar_position: 11
---

A decision model scores predefined answers instead of generating text. When you need to classify text and already know the possible answers, these models are a good choice.

NobodyWho currently supports only [Laya GGUF](https://huggingface.co/ggml-org/Laya-GGUF) decision models. Every decision runs non-generatively, so `usage.outputTokens` is always `0`.

## Load a model

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final decision = await nobodywho.Decision.fromPath(
  modelPath: Platform.environment['TEST_DECISION_MODEL'] ?? 'Laya-Q8_0.gguf',
);
```

To reuse an already loaded model, pass it to the constructor: `nobodywho.Decision(model: model, nCtx: 1024)`. The context size defaults to 1024.

## Make a decision

`decide` takes two named arguments:

- `state`: the text or JSON-compatible data to evaluate.
- `questions`: a map of question IDs. Each question has a `type`, `instructions`, and any `criteria` the type requires.

```dart continuation
final message = 'I was charged twice. Please refund the duplicate payment.';
final questions = {
  'department': {
    'type': 'choice',
    'instructions': 'Which team should handle this?',
    'criteria': {
      'billing': 'Payments and refunds',
      'technical': 'Bugs and login problems',
    },
  },
  'refund': {
    'type': 'noul',
    'instructions': 'Is a refund requested?',
  },
  'urgency': {
    'type': 'score',
    'instructions': 'How urgent is this?',
    'criteria': ['can wait', 'soon', 'urgent'],
  },
};

final result = await decision.decide(
  state: {'message': message},
  questions: questions,
);
final answers = (result as Map)['answers'] as Map;
final team = (answers['department'] as Map)['choice'] as String;
final refundProbability = (answers['refund'] as Map)['noul'] as double;
final urgency = (answers['urgency'] as Map)['score'] as double;
print('$team, refund $refundProbability, urgency $urgency');
```

## Choose a question type

### Select an option with `choice`

To route an email to a team, use `choice`. A `choice` value of `"billing"` selects that team. Read `probabilities` for the probability of each option.

### Check a condition with `noul`

To check whether the sender requests a refund, use `noul`. A `noul` value of `0.9` means the model estimates a 90% probability that the answer is yes.

### Rate content with `score`

Use `score` when the answers form an ordered scale. Define two to ten descriptive levels in `criteria`, from lowest to highest.

The score is a weighted average, not a probability. To find the probability of `urgent`, read `probabilities["2"]`. The `legend` maps level IDs to their descriptions.

## Read the return values

`decide` returns a `Map` with `answers` and `usage`. Question IDs and option IDs keep their order.

- `answers`: an answer map for each question ID. Each answer contains the fields for its question type:
  - `type`: `choice`, `noul`, or `score`.
  - `choice`: the selected option ID.
  - `noul`: the probability that the answer is yes, from 0 to 1.
  - `score`: the probability-weighted average of the zero-based levels.
  - `probabilities`: the probability of each option or level, for `choice` and `score`.
  - `confidence`: how concentrated the probabilities are, from 0 to 1. Only `choice` and `score` return `confidence`. High confidence does not guarantee a correct answer.
  - `legend`: level IDs mapped to their descriptions, for `score`.
- `usage`: token counts.
  - `input_tokens`: the number of input tokens evaluated across all questions.
  - `output_tokens`: always `0`.

## Configure GPU use

Use the same model and GPU settings as chat. `Decision.fromPath` accepts `useGpu` (default `true`), and `onDownloadProgress` reports download progress with `(downloaded, total)` byte counts.
