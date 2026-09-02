.PHONY: help model build verify benchmark clean

MAKEFILE_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
MODEL_DIR ?= $(abspath $(MAKEFILE_DIR)/../../models/gemma-4-E2B-it)
PROMPT_TOKENS ?= 512
GENERATION_TOKENS ?= 128
GREEDY_TOKENS ?= 8
TOKENS ?= 32
PROMPT ?= The capital of Denmark is
FLASH_ATTN ?= on
REPETITIONS ?= 5

help:
	@echo 'make model | build | verify | benchmark | clean'

model:
	./download-model.sh "$(MODEL_DIR)"

build:
	cargo build --release

verify:
	MODEL_DIR="$(MODEL_DIR)" TOKENS="$(TOKENS)" PROMPT="$(PROMPT)" FLASH_ATTN="$(FLASH_ATTN)" ./verify.sh

benchmark: build
	MODEL_DIR="$(MODEL_DIR)" \
	PROMPT_TOKENS="$(PROMPT_TOKENS)" \
	GENERATION_TOKENS="$(GENERATION_TOKENS)" \
	GREEDY_TOKENS="$(GREEDY_TOKENS)" \
	FLASH_ATTN="$(FLASH_ATTN)" \
	REPETITIONS="$(REPETITIONS)" \
	./benchmark-metal.sh

clean:
	cargo clean --package ggml-gemma4-poc
