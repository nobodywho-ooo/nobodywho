.PHONY: help model build run run-cpu clean

MODEL_DIR ?= models/gemma-4-E2B-it
PROMPT ?= Explain why the sky is blue in one sentence.
MAX_TOKENS ?= 8

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
	cargo clean --package ggml-gemma4-poc
