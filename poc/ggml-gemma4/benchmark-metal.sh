#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/../.." && pwd)"
model_dir="${MODEL_DIR:-$repo_root/models/gemma-4-E2B-it}"
model="$model_dir/gemma-4-E2B-it-Q4_K_M.gguf"
prompt_tokens="${PROMPT_TOKENS:-512}"
generation_tokens="${GENERATION_TOKENS:-128}"
repetitions="${REPETITIONS:-5}"
greedy_tokens="${GREEDY_TOKENS:-8}"
flash_attention="${FLASH_ATTN:-on}"
if ! [[ "$generation_tokens" =~ ^[1-9][0-9]*$ && "$greedy_tokens" =~ ^[1-9][0-9]*$ ]]; then
  echo 'Generation and greedy token counts must be positive integers.' >&2
  exit 1
fi
if ((greedy_tokens > generation_tokens)); then
  echo 'Greedy token count cannot exceed the generation token count.' >&2
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
llama_prefix="$(pkg-config --variable=prefix llama)"
llama_bench="$llama_prefix/bin/llama-bench"
if [[ ! -x "$llama_bench" ]]; then
  echo "llama-bench was not found in the pkg-config llama installation: $llama_prefix" >&2
  exit 1
fi

if [[ -n "${OUTPUT_DIR:-}" ]]; then
  output_dir="$OUTPUT_DIR"
  mkdir -p "$output_dir"
else
  output_dir="$(mktemp -d)"
  trap 'rm -rf "$output_dir"' EXIT
fi

cargo build --release --manifest-path "$repo_root/poc/Cargo.toml" -p ggml-gemma4-poc
read -r -a llama_flags <<< "$(pkg-config --cflags --libs llama ggml)"
c++ -std=c++17 "$script_dir/llama-greedy.cpp" "${llama_flags[@]}" -o "$output_dir/llama-greedy"
direct_flags=()
if [[ "$flash_attention" == "on" ]]; then
  direct_flags+=(--flash-attention)
fi

"$repo_root/poc/target/release/ggml-gemma4-poc" \
  "${direct_flags[@]}" \
  --model-dir "$model_dir" \
  --prompt-tokens "$prompt_tokens" \
  --generation-tokens "$generation_tokens" \
  --repetitions "$repetitions" \
  --greedy-tokens "$greedy_tokens" \
  --greedy-tokens-output "$output_dir/direct-greedy.json" \
  --json > "$output_dir/direct-ggml.json"

"$output_dir/llama-greedy" \
  "$model" \
  "$greedy_tokens" \
  "$generation_tokens" \
  "$flash_attention" > "$output_dir/llama-greedy.json" \
  2> "$output_dir/llama-greedy.log"
if [[ "$(jq -c . "$output_dir/direct-greedy.json")" != "$(jq -c . "$output_dir/llama-greedy.json")" ]]; then
  echo 'Greedy token validation failed:' >&2
  echo "  direct GGML: $(jq -c . "$output_dir/direct-greedy.json")" >&2
  echo "  llama.cpp:   $(jq -c . "$output_dir/llama-greedy.json")" >&2
  exit 1
fi

"$llama_bench" \
  --model "$model" \
  --n-prompt "$prompt_tokens" \
  --n-gen "$generation_tokens" \
  --batch-size "$prompt_tokens" \
  --ubatch-size "$prompt_tokens" \
  --repetitions "$repetitions" \
  --n-gpu-layers 99 \
  --flash-attn "$flash_attention" \
  --cache-type-k f16 \
  --cache-type-v f16 \
  --output json > "$output_dir/llama-cpp.json"

prompt_name="pp$prompt_tokens"
generation_name="tg$generation_tokens"
direct_prompt="$(jq -r --arg test "$prompt_name" '.[] | select(.test == $test) | .avg_ts' "$output_dir/direct-ggml.json")"
direct_generation="$(jq -r --arg test "$generation_name" '.[] | select(.test == $test) | .avg_ts' "$output_dir/direct-ggml.json")"
llama_prompt="$(jq -r --argjson count "$prompt_tokens" '.[] | select(.n_prompt == $count and .n_gen == 0) | .avg_ts' "$output_dir/llama-cpp.json")"
llama_generation="$(jq -r --argjson count "$generation_tokens" '.[] | select(.n_prompt == 0 and .n_gen == $count) | .avg_ts' "$output_dir/llama-cpp.json")"
prompt_ratio="$(awk -v direct="$direct_prompt" -v llama="$llama_prompt" 'BEGIN { printf "%.3f", direct / llama }')"
generation_ratio="$(awk -v direct="$direct_generation" -v llama="$llama_generation" 'BEGIN { printf "%.3f", direct / llama }')"

printf '\nGreedy validation: %s matching tokens\n' "$greedy_tokens"
printf 'Flash attention: %s\n' "$flash_attention"
printf '\n| test | direct GGML t/s | llama.cpp t/s | direct / llama.cpp |\n'
printf '| --- | ---: | ---: | ---: |\n'
printf '| %s | %.2f | %.2f | %s |\n' "$prompt_name" "$direct_prompt" "$llama_prompt" "$prompt_ratio"
printf '| %s | %.2f | %.2f | %s |\n' "$generation_name" "$direct_generation" "$llama_generation" "$generation_ratio"

if [[ -n "${OUTPUT_DIR:-}" ]]; then
  printf '\nRaw JSON: %s\n' "$output_dir"
fi
