# /// script
# requires-python = ">=3.10"
# dependencies = ["nobodywho"]
#
# [tool.uv.sources]
# nobodywho = { path = "nobodywho/python" }
# ///

"""Run the NobodyWho Mimir ONNX prototype from Hugging Face."""

import nobodywho

mimir = nobodywho.Mimir(
    source="hf://duarteocarmo/DFM-Mimir-ONNX",
    model_file="onnx/model_int8.onnx",
    max_new_tokens=64,
    device="cpu",
)

for piece in mimir.ask(prompt="Hvad er Danmarks hovedstad?"):
    print(piece, end="", flush=True)
print()
