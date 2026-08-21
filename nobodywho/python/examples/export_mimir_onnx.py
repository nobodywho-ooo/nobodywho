# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "huggingface-hub>=0.36",
#   "numpy>=2",
#   "onnx>=1.20",
#   "onnx-ir>=0.1.15",
#   "onnxruntime==1.29.0",
#   "torch>=2.8",
#   "transformers==5.15.1",
# ]
#
# [tool.uv]
# exclude-newer = "2026-08-20T00:00:00Z"
# ///

"""Build a prototype Mimir ONNX model from the compatible HRM-Text graph."""

import argparse
import logging
import re
import shutil
from pathlib import Path

import numpy
import onnx  # ty: ignore[unresolved-import]
import torch  # ty: ignore[unresolved-import]
from huggingface_hub import hf_hub_download, snapshot_download  # ty: ignore[unresolved-import]
from onnxruntime.quantization.matmul_nbits_quantizer import (  # ty: ignore[unresolved-import]
    MatMulNBitsQuantizer,
)
from transformers import AutoModelForCausalLM  # ty: ignore[unresolved-import]

MODEL_ID = "danish-foundation-models/DFM-Mimir"
TEMPLATE_ID = "onnx-community/HRM-Text-1B-ONNX"
CHUNK_SIZE = 2_000_000_000


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--model", default=MODEL_ID)
    return parser.parse_args()


def tensor_for(name: str, state: dict[str, torch.Tensor], config) -> torch.Tensor:
    if name == "model.embed_tokens.weight":
        return state[name]
    if name == "model.z_L_init":
        return state[name]
    if name == "model.hrm_text.rmsnorm.weight":
        return torch.ones(config.hidden_size)
    if name in {"cos_cache", "sin_cache"}:
        positions = torch.arange(config.max_position_embeddings, dtype=torch.float32)
        dimensions = torch.arange(0, config.head_dim, 2, dtype=torch.float32)
        inverse_frequency = 1.0 / (
            config.rope_parameters["rope_theta"] ** (dimensions / config.head_dim)
        )
        frequencies = torch.outer(positions, inverse_frequency)
        return frequencies.cos() if name == "cos_cache" else frequencies.sin()
    if name == "lm_head.MatMul.weight":
        return state["lm_head.weight"].transpose(0, 1)

    match = re.fullmatch(
        r"model\.(L_module|H_module)\.layers\.(\d+)\.(.+)\.weight", name
    )
    if not match:
        raise ValueError(f"unsupported template initializer: {name}")
    stack, layer, operation = match.groups()
    prefix = f"model.{stack}.layers.{layer}"
    if operation == "self_attn.qkvg_proj":
        weights = [
            state[f"{prefix}.self_attn.{projection}_proj.weight"]
            for projection in ("q", "k", "v", "gate")
        ]
        return torch.cat(weights, dim=0).transpose(0, 1)
    if operation == "self_attn.o_proj":
        return state[f"{prefix}.{operation}.weight"].transpose(0, 1)
    if operation == "mlp.gate_up_proj":
        weights = [
            state[f"{prefix}.mlp.{projection}_proj.weight"]
            for projection in ("gate", "up")
        ]
        return torch.cat(weights, dim=0).transpose(0, 1)
    if operation == "mlp.down_proj":
        return state[f"{prefix}.{operation}.weight"].transpose(0, 1)
    raise ValueError(f"unsupported template initializer: {name}")


def update_value_shapes(model: onnx.ModelProto, vocab_size: int) -> None:
    shapes = {
        "model.embed_tokens.weight": [vocab_size, 1536],
        "lm_head.MatMul.weight": [1536, vocab_size],
        "logits": ["batch_size", "num_logits_to_keep", vocab_size],
    }
    for value in [*model.graph.input, *model.graph.output, *model.graph.value_info]:
        shape = shapes.get(value.name)
        if shape is None:
            continue
        dimensions = value.type.tensor_type.shape.dim
        del dimensions[:]
        for item in shape:
            dimension = dimensions.add()
            if isinstance(item, int):
                dimension.dim_value = item
            else:
                dimension.dim_param = item


def copy_model_files(model_source: str, output: Path) -> None:
    snapshot = Path(
        snapshot_download(
            repo_id=model_source,
            allow_patterns=[
                "config.json",
                "tokenizer.json",
                "tokenizer_config.json",
                "chat_template.jinja",
            ],
        )
    )
    for name in (
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "chat_template.jinja",
    ):
        shutil.copy2(snapshot / name, output / name)


def set_external_location(initializer, location: str, offset: int, length: int) -> None:
    initializer.ClearField("external_data")
    for key, value in (("location", location), ("offset", offset), ("length", length)):
        entry = initializer.external_data.add()
        entry.key = key
        entry.value = str(value)
    initializer.data_location = onnx.TensorProto.EXTERNAL


def export(model_source: str, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    onnx_dir = output / "onnx"
    onnx_dir.mkdir(exist_ok=True)
    for pattern in ("model_fp16.onnx*", "model_int8.onnx*"):
        for stale in onnx_dir.glob(pattern):
            stale.unlink()
    copy_model_files(model_source=model_source, output=output)

    template_path = hf_hub_download(
        repo_id=TEMPLATE_ID, filename="onnx/model_fp16.onnx"
    )
    graph = onnx.load_model(template_path, load_external_data=False)
    model = AutoModelForCausalLM.from_pretrained(
        model_source,
        dtype=torch.bfloat16,
        attn_implementation="sdpa",
    )
    model.eval()
    state = model.state_dict()

    external_initializers = [
        initializer
        for initializer in graph.graph.initializer
        if initializer.external_data
    ]
    chunk_index = 0
    offset = 0
    file = None
    try:
        for initializer in external_initializers:
            tensor = tensor_for(name=initializer.name, state=state, config=model.config)
            tensor = tensor.detach().to(dtype=torch.float16).contiguous()
            byte_length = tensor.numel() * tensor.element_size()
            if file is None or (offset and offset + byte_length > CHUNK_SIZE):
                if file is not None:
                    file.close()
                filename = "model_fp16.onnx_data" + (
                    f"_{chunk_index}" if chunk_index else ""
                )
                file = (onnx_dir / filename).open("wb")
                chunk_index += 1
                offset = 0
            array = tensor.numpy().astype(numpy.float16, copy=False)
            array.tofile(file)
            initializer.dims[:] = tensor.shape
            set_external_location(
                initializer=initializer,
                location=Path(file.name).name,
                offset=offset,
                length=byte_length,
            )
            offset += byte_length
            del array, tensor
    finally:
        if file is not None:
            file.close()

    update_value_shapes(model=graph, vocab_size=model.config.vocab_size)
    graph_path = onnx_dir / "model_fp16.onnx"
    onnx.save_model(graph, graph_path)
    onnx.checker.check_model(graph_path)

    logging.getLogger("onnxruntime.quantization.matmul_nbits_quantizer").setLevel(
        logging.WARNING
    )
    quantized_path = onnx_dir / "model_int8.onnx"
    quantizer = MatMulNBitsQuantizer(
        model=str(graph_path),
        bits=8,
        block_size=128,
        is_symmetric=True,
        op_types_to_quantize=("MatMul",),
    )
    quantizer.process()
    quantizer.model.save_model_to_file(str(quantized_path), True)
    onnx.checker.check_model(quantized_path)
    for fp16_file in onnx_dir.glob("model_fp16.onnx*"):
        fp16_file.unlink()
    print(f"Exported INT8 Mimir ONNX model to {output}")


def main() -> None:
    args = arguments()
    export(model_source=args.model, output=args.output)


if __name__ == "__main__":
    main()
