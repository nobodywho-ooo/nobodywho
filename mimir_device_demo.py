# /// script
# requires-python = ">=3.10"
# dependencies = [
#   "nobodywho",
#   "onnxruntime==1.27.0; platform_system == 'Darwin' and platform_machine == 'arm64'",
#   "onnxruntime-ep-mlx==0.27.6; platform_system == 'Darwin' and platform_machine == 'arm64'",
# ]
#
# [tool.uv.sources]
# nobodywho = { path = "nobodywho/python", editable = true }
# ///

"""Compare NobodyWho Mimir inference on CPU and Apple MPS."""

import argparse

import nobodywho

MODEL_SOURCE = "hf://duarteocarmo/DFM-Mimir-ONNX"
PROMPT = "What's the capital of Le Marche region?"


def run_inference(device: str) -> None:
    print(f"\n{device.upper()}:")
    mimir = nobodywho.Mimir(
        source=MODEL_SOURCE,
        max_new_tokens=64,
        device=device,
    )
    for piece in mimir.ask(prompt=PROMPT):
        print(piece, end="", flush=True)
    print()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--device",
        choices=("cpu", "mps", "both"),
        default="both",
    )
    arguments = parser.parse_args()
    devices = ("cpu", "mps") if arguments.device == "both" else (arguments.device,)
    for device in devices:
        run_inference(device=device)


if __name__ == "__main__":
    main()
