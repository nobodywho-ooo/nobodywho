#!/usr/bin/env bash
set -euo pipefail

model_revision='c0ccefd2b9e457d7eee1ba122c650fc8ca6e189c'
asset_revision='3cadd1ee6394adea1bd021217a0e650ede09a323'
model_base="https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/$model_revision/Supertonic-3-GGUF"
asset_base="https://huggingface.co/Supertone/supertonic-3/resolve/$asset_revision"
out="${1:-models/supertonic-3}"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

mkdir -p "$out/voice_styles"
curl --fail --location --continue-at - \
  --output "$out/supertonic-3-orig.gguf" \
  "$model_base/supertonic-3-orig.gguf"
curl --fail --location --output "$out/tts.json" "$asset_base/onnx/tts.json"
curl --fail --location --output "$out/unicode_indexer.json" "$asset_base/onnx/unicode_indexer.json"
for voice in M1 M2 M3 M4 M5 F1 F2 F3 F4 F5; do
  curl --fail --location \
    --output "$out/voice_styles/$voice.json" \
    "$asset_base/voice_styles/$voice.json"
done

if command -v sha256sum >/dev/null; then
  (cd "$out" && sha256sum --check "$script_dir/SHA256SUMS")
else
  (cd "$out" && shasum -a 256 --check "$script_dir/SHA256SUMS")
fi

echo "Downloaded and verified Supertonic assets in $out"
