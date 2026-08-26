.PHONY: help model build run run-metal clean

MODEL_DIR ?= models/supertonic-3
TEXT ?= Hello from NobodyWho.
OUTPUT ?= output.wav

help:
	@echo 'make model | build | run | run-metal | clean'

model:
	./download-model.sh "$(MODEL_DIR)"

build:
	cargo build --release

run: build
	cargo run --release -- --model-dir "$(MODEL_DIR)" --backend cpu --text "$(TEXT)" --output "$(OUTPUT)"

run-metal: build
	cargo run --release -- --model-dir "$(MODEL_DIR)" --backend metal --text "$(TEXT)" --output "$(OUTPUT)"

clean:
	cargo clean --package ggml-supertonic-poc
