#!/usr/bin/env bash
set -euo pipefail

model_revision='0314792d7f1f7e229411f620751375812bb9faf2'
assets_revision='3e22461f65e89153144f8adb70e3b8c2cc9845a7'
model_base="https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/$model_revision"
assets_base="https://huggingface.co/google/gemma-4-E2B-it/resolve/$assets_revision"
out="${1:-models/gemma-4-E2B-it}"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$out"

curl --fail --location --continue-at - \
  --output "$out/gemma-4-E2B-it-Q4_K_M.gguf" \
  "$model_base/gemma-4-E2B-it-Q4_K_M.gguf"
for file in config.json generation_config.json tokenizer.json; do
  curl --fail --location --output "$out/$file" "$assets_base/$file"
done

if command -v sha256sum >/dev/null; then
  (cd "$out" && sha256sum --check "$script_dir/SHA256SUMS")
else
  (cd "$out" && shasum -a 256 --check "$script_dir/SHA256SUMS")
fi

echo "Downloaded and verified Gemma 4 E2B assets in $out"
