#!/bin/sh
set -e

if ! command -v just > /dev/null; then
    echo "Installing just..."
    cargo install just
fi

git config core.hooksPath .githooks
chmod +x .githooks/pre-push

echo "Done. Pre-push hook installed."
