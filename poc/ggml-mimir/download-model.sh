#!/usr/bin/env bash
set -euo pipefail

revision='2844f0178e695d7d9ce182cb660671fd34c76ce5'
base="https://huggingface.co/danish-foundation-models/DFM-Mimir/resolve/$revision"
out="${1:-models/DFM-Mimir}"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$out"

curl --fail --location --continue-at - --output "$out/model.safetensors" "$base/model.safetensors"
for file in config.json tokenizer.json tokenizer_config.json chat_template.jinja LICENSE; do
  curl --fail --location --output "$out/$file" "$base/$file"
done

if command -v sha256sum >/dev/null; then
  (cd "$out" && sha256sum --check "$script_dir/SHA256SUMS")
else
  (cd "$out" && shasum -a 256 --check "$script_dir/SHA256SUMS")
fi

echo "Downloaded and verified Mimir assets in $out"
