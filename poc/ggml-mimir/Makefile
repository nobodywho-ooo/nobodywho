.PHONY: help model build run run-cpu clean

MODEL_DIR ?= models/DFM-Mimir
PROMPT ?= Explain why the sky is blue in one sentence.
MAX_TOKENS ?= 32

help:
	@echo 'make model | build | run | run-cpu | clean'

model:
	./download-model.sh "$(MODEL_DIR)"

build:
	cargo build --release

run: build
	cargo run --release -- --model-dir "$(MODEL_DIR)" --backend metal --prompt "$(PROMPT)" --max-tokens "$(MAX_TOKENS)"

run-cpu: build
	cargo run --release -- --model-dir "$(MODEL_DIR)" --backend cpu --prompt "$(PROMPT)" --max-tokens "$(MAX_TOKENS)"

clean:
	cargo clean --package ggml-mimir-poc
