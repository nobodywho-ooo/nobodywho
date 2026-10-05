# A container with everything needed to build and test the C# binding: Rust, the
# llama.cpp build dependencies (as in CI's Linux build), the .NET 10 SDK and the
# pinned uniffi-bindgen-cs. An alternative to `nix develop`.
#
#   docker build -t nobodywho-csharp-dev -f nobodywho/csharp/dev.Dockerfile .
#   docker run --rm -it -v "$PWD:/src" -w /src/nobodywho nobodywho-csharp-dev
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl git pkg-config libssl-dev \
      clang libclang-dev cmake ninja-build \
      libshaderc-dev libvulkan-dev glslc mesa-vulkan-drivers \
      libicu74 python3 \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:/opt/dotnet:$PATH \
    DOTNET_ROOT=/opt/dotnet DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup component add rustfmt clippy
RUN curl -sSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
    && bash /tmp/dotnet-install.sh --channel 10.0 --install-dir /opt/dotnet && rm /tmp/dotnet-install.sh
RUN curl -sSL https://just.systems/install.sh | bash -s -- --to /usr/local/bin
# Keep in step with BINDGEN_REV in scripts/generate-bindings.sh.
RUN cargo install uniffi-bindgen-cs --locked \
      --git https://github.com/NordSecurity/uniffi-bindgen-cs \
      --rev 1f677ed60839cf9cf7fbcdd305b2eeaae29d84c3
RUN git config --global --add safe.directory '*'
WORKDIR /src
