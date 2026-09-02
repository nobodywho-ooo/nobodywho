#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"
model_dir="${MODEL_DIR:-$repo_root/models/gemma-4-E2B-it}"
model="$model_dir/gemma-4-E2B-it-Q4_K_M.gguf"
token_count="${TOKENS:-32}"
prompt="${PROMPT:-Some interesting facts about Portugal: }"
flash_attention="${FLASH_ATTN:-on}"

if ! [[ "$token_count" =~ ^[1-9][0-9]*$ ]]; then
  echo 'TOKENS must be a positive integer.' >&2
  exit 1
fi
if [[ "$flash_attention" != "on" && "$flash_attention" != "off" ]]; then
  echo 'FLASH_ATTN must be on or off.' >&2
  exit 1
fi
if [[ ! -f "$model" || ! -f "$model_dir/config.json" ]]; then
  echo "Missing Gemma 4 E2B model assets in $model_dir" >&2
  echo "Run 'make model' from $script_dir first." >&2
  exit 1
fi
for command in cargo c++ jq pkg-config; do
  if ! command -v "$command" >/dev/null; then
    echo "Required command not found: $command" >&2
    exit 1
  fi
done

output_dir="$(mktemp -d)"
trap 'rm -rf "$output_dir"' EXIT

cargo build --quiet --release --manifest-path "$repo_root/poc/Cargo.toml" -p ggml-gemma4-poc
read -r -a llama_flags <<< "$(pkg-config --cflags --libs llama ggml)"
c++ -std=c++17 -O2 "$script_dir/llama-greedy.cpp" "${llama_flags[@]}" -o "$output_dir/llama-greedy"
llama_tokenize="$(pkg-config --variable=prefix llama)/bin/llama-tokenize"
if [[ ! -x "$llama_tokenize" ]]; then
  echo "llama-tokenize was not found at $llama_tokenize" >&2
  exit 1
fi
if ! "$llama_tokenize" -m "$model" -p "$prompt" --ids > "$output_dir/prompt-tokens.json" \
  2> "$output_dir/tokenize.log"; then
  cat "$output_dir/tokenize.log" >&2
  exit 1
fi
prompt_token_count="$(jq 'length' "$output_dir/prompt-tokens.json")"
context_size="$((prompt_token_count + token_count))"

direct_flags=()
if [[ "$flash_attention" == "on" ]]; then
  direct_flags+=(--flash-attention)
fi

if ! "$repo_root/poc/target/release/ggml-gemma4-poc" \
  "${direct_flags[@]}" \
  --model-dir "$model_dir" \
  --prompt-tokens 1 \
  --generation-tokens "$context_size" \
  --repetitions 1 \
  --greedy-tokens "$token_count" \
  --greedy-tokens-output "$output_dir/direct-tokens.json" \
  --greedy-report-output "$output_dir/direct.json" \
  --greedy-prompt-tokens "$output_dir/prompt-tokens.json" \
  --greedy-only 2> "$output_dir/direct.log"; then
  cat "$output_dir/direct.log" >&2
  exit 1
fi

if ! "$output_dir/llama-greedy" \
  "$model" \
  "$token_count" \
  "$context_size" \
  "$flash_attention" \
  "$output_dir/direct-tokens.json" \
  "$output_dir/prompt-tokens.json" > "$output_dir/llama.json" \
  2> "$output_dir/llama.log"; then
  cat "$output_dir/llama.log" >&2
  exit 1
fi

direct_tokens="$(jq -c '.tokens' "$output_dir/direct.json")"
llama_tokens="$(jq -c '.tokens' "$output_dir/llama.json")"
if [[ "$direct_tokens" == "$llama_tokens" ]]; then
  parity='exact match'
else
  parity='MISMATCH'
fi

printf 'Gemma 4 E2B greedy inference\n'
printf 'Prompt: %s\n' "$prompt"
printf 'Maximum completion tokens: %s | Flash attention: %s | F16 KV | Metal\n' "$token_count" "$flash_attention"
printf 'Timing includes prompt ingestion and generation. It excludes model loading, warmup, tokenization, and text decoding.\n\n'

printf 'Direct GGML\n'
printf '  Generated tokens: %s\n' "$direct_tokens"
printf '  Completion:\n%s\n' "$(jq -r '.direct_completion' "$output_dir/llama.json")"
printf '  Total latency: %.2f ms\n' "$(jq -r '.latency_ms' "$output_dir/direct.json")"
printf '  Throughput: %.2f tokens/s\n\n' "$(jq -r '.tokens_per_second' "$output_dir/direct.json")"

printf 'llama.cpp\n'
printf '  Generated tokens: %s\n' "$llama_tokens"
printf '  Completion:\n%s\n' "$(jq -r '.completion' "$output_dir/llama.json")"
printf '  Total latency: %.2f ms\n' "$(jq -r '.latency_ms' "$output_dir/llama.json")"
printf '  Throughput: %.2f tokens/s\n\n' "$(jq -r '.tokens_per_second' "$output_dir/llama.json")"

printf 'Token parity: %s\n' "$parity"
if [[ "$parity" == 'MISMATCH' ]]; then
  exit 1
fi
