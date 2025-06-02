#!/bin/bash

set -e

cd "$(dirname "$0")"

# Build base image
docker build -t local-gameci-base:latest ubuntu/base/

# Build hub image
docker build --build-arg baseImage=local-gameci-base:latest -t local-gameci-hub:latest ubuntu/hub/

# Build editor image
docker build \
    --build-arg baseImage=local-gameci-base:latest \
    --build-arg hubImage=local-gameci-hub:latest \
    --build-arg version=6000.0.47f1 \
    --build-arg changeSet=8a060a5ff2be \
    --build-arg module=linux-il2cpp \
    -t local-gameci-editor:latest \
    ubuntu/editor/

# Build test-runner-vulkan image
docker build -t nobodywho-unity-ci:latest -f gameci-test-runner-vulkan .


# if [ -n "$RELEASE" ]; then
#    
#     docker push nobodywho/nobodywho-unity-ci:latest
# fi
