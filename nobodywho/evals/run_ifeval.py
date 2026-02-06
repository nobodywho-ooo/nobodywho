import json
import os
import sys

import ifeval
from nobodywho import Chat
from tqdm import tqdm

dataset_file = "ifeval_input_data.jsonl"
responses_file = "responses.jsonl"

model_file_arg: str | None = sys.argv[1] if len(sys.argv) > 1 else None
model_file = model_file_arg or os.getenv("TEST_MODEL")
assert isinstance(model_file, str), "Must provide model file"


def generate_responses():
    assert isinstance(model_file, str), "Must provide model file"
    chat = Chat(model_file)

    results = []
    lines = [line for line in open(dataset_file)]
    for line in tqdm(lines):
        task = json.loads(line)

        chat.reset_history()
        response: str = chat.ask(task["prompt"]).completed()
        results.append({"prompt": task["prompt"], "response": response})

    jsonl_str = "\n".join(json.dumps(r) for r in results)
    with open(responses_file, "w") as f:
        f.write(jsonl_str)


def evaluate():
    evaluator = ifeval.Evaluator(ifeval.instruction_registry)
    input_examples = ifeval.read_input_examples(dataset_file)
    responses = ifeval.read_responses(responses_file)
    report, all_outputs = evaluator.evaluate(input_examples, responses)
    print("Strict prompt accuracy:", report["eval_results_strict"]["prompt_accuracy"])
    print("Loose prompt accuracy:", report["eval_results_loose"]["prompt_accuracy"])


if __name__ == "__main__":
    generate_responses()
    evaluate()
