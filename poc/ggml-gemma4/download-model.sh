#!/usr/bin/env bash
set -euo pipefail

model_revision='0314792d7f1f7e229411f620751375812bb9faf2'
assets_revision='3e22461f65e89153144f8adb70e3b8c2cc9845a7'
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
out="${1:-$script_dir/../../models/gemma-4-E2B-it}"

if ! command -v hf >/dev/null; then
  echo 'The hf CLI is required to download the model.' >&2
  exit 1
fi

hf download unsloth/gemma-4-E2B-it-GGUF \
  gemma-4-E2B-it-Q4_K_M.gguf \
  --revision "$model_revision" \
  --local-dir "$out"
hf download google/gemma-4-E2B-it \
  config.json \
  --revision "$assets_revision" \
  --local-dir "$out"

if command -v sha256sum >/dev/null; then
  (cd "$out" && sha256sum --check "$script_dir/SHA256SUMS")
else
  (cd "$out" && shasum -a 256 --check "$script_dir/SHA256SUMS")
fi

echo "Downloaded and verified Gemma 4 E2B assets in $out"
