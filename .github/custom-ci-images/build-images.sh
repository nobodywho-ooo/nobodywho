#!/bin/bash

set -e

cd "$(dirname "$0")"

# Build base image
docker build -t local-gameci-base:latest ubuntu/base/

# Build hub image
docker build --build-arg baseImage=local-gameci-base:latest -t local-gameci-hub:latest ubuntu/hub/

# Build editor image
docker build --build-arg baseImage=local-gameci-base:latest --build-arg hubImage=local-gameci-hub:latest -t local-gameci-editor:latest ubuntu/editor/

# Build test-runner-vulkan image
docker build -t local-gameci-testrunner-vulkan:latest -f gameci-test-runner-vulkan .