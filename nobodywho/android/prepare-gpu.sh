#!/usr/bin/env bash
# Build Android GPU dependencies. stdout contains environment assignments;
# build output goes to stderr. Run from any directory; see README.md.
set -euo pipefail

target="${1:?usage: prepare-gpu.sh <Rust Android target> [--github-env]}"
case "$target" in
  aarch64-linux-android) abi=arm64-v8a ;;
  x86_64-linux-android) abi=x86_64 ;;
  *) echo "Unsupported Android target: $target" >&2; exit 1 ;;
esac
: "${ANDROID_NDK:?Set ANDROID_NDK to the Android NDK directory}"
case "$(uname -s)" in
  Darwin) host_tag=darwin-x86_64 ;;
  Linux) host_tag=linux-x86_64 ;;
  *) echo "Run this helper on Linux or macOS" >&2; exit 1 ;;
esac
glslc="${VULKAN_GLSLC:-$ANDROID_NDK/shader-tools/$host_tag/glslc}"
[[ -x "$glslc" ]] || { echo "Set VULKAN_GLSLC to a host glslc executable" >&2; exit 1; }
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
deps_dir="${NOBODYWHO_ANDROID_GPU_DIR:-$script_dir/../target/android-gpu}"
mkdir -p "$deps_dir"
deps_dir="$(cd "$deps_dir" && pwd)"

# Immutable revisions, shared between the two Android ABI builds.
loader_rev=ad770a1b64c6b8d5f2ed4e153f22e4f45939f27f # v2025.07.22
headers_rev=8a97ebc88daa3495d6f57ec10bb515224400186f # v2025.07.22
vulkan_rev=409c16be502e39fe70dd6fe2d9ad4842ef2c9a53 # v1.4.313
spirv_rev=aa6cef192b8e693916eb713e7a9ccadf06062ceb # SDK 1.4.313.0

fetch_source() {
  local repo="$1" revision="$2" destination="$3"
  if [[ ! -d "$destination" ]]; then
    local staging
    staging="$(mktemp -d "$deps_dir/download.XXXXXX")"
    curl --fail --location --retry 3 \
      "https://codeload.github.com/KhronosGroup/$repo/tar.gz/$revision" \
      -o "$staging/source.tar.gz"
    mkdir "$staging/source"
    tar -xzf "$staging/source.tar.gz" -C "$staging/source" --strip-components=1
    if [[ "$repo" == OpenCL-ICD-Loader ]]; then
      patch -d "$staging/source" -p1 < "$script_dir/opencl-android.patch"
    fi
    mv "$staging/source" "$destination"
    rm "$staging/source.tar.gz"
    rmdir "$staging"
  fi
}

patch_hash="$(shasum -a 256 "$script_dir/opencl-android.patch" | cut -d ' ' -f 1)"
loader="$deps_dir/OpenCL-ICD-Loader-$loader_rev-$patch_hash"
headers="$deps_dir/OpenCL-Headers-$headers_rev"
vulkan="$deps_dir/Vulkan-Headers-$vulkan_rev"
spirv="$deps_dir/SPIRV-Headers-$spirv_rev"
fetch_source OpenCL-ICD-Loader "$loader_rev" "$loader" >&2
fetch_source OpenCL-Headers "$headers_rev" "$headers" >&2
fetch_source Vulkan-Headers "$vulkan_rev" "$vulkan" >&2
fetch_source SPIRV-Headers "$spirv_rev" "$spirv" >&2

build_dir="$deps_dir/$target/$loader_rev-$patch_hash"
cmake -S "$loader" -B "$build_dir" \
  -DCMAKE_TOOLCHAIN_FILE="$ANDROID_NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI="$abi" -DANDROID_PLATFORM=24 \
  -DCMAKE_BUILD_TYPE=Release \
  -DOPENCL_ICD_LOADER_HEADERS_DIR="$headers" \
  -DOPENCL_ICD_LOADER_BUILD_SHARED_LIBS=OFF \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DCMAKE_C_VISIBILITY_PRESET=hidden \
  -DENABLE_OPENCL_LAYERS=OFF -DBUILD_TESTING=OFF >&2
cmake --build "$build_dir" --target OpenCL --parallel 4 >&2

# SPIRV-Headers' CMake package is needed by ggml-vulkan when cross-compiling.
cmake -S "$spirv" -B "$deps_dir/spirv-build" \
  -DCMAKE_INSTALL_PREFIX="$deps_dir/spirv-install" >&2
cmake --install "$deps_dir/spirv-build" >&2

emit() {
  if [[ "${2-}" == --github-env ]]; then
    printf '%s=%s\n' "$1" "$3"
  else
    printf 'export %s=%q\n' "$1" "$3"
  fi
}
emit OPENCL_INCLUDE_DIR "${2-}" "$headers"
emit OPENCL_LIBRARY "${2-}" "$build_dir/libOpenCL.a"
emit VULKAN_INCLUDE_DIR "${2-}" "$vulkan/include"
emit VULKAN_GLSLC "${2-}" "$glslc"
emit SPIRV_HEADERS_DIR "${2-}" "$deps_dir/spirv-install/share/cmake/SPIRV-Headers"
emit SPIRV_HEADERS_INCLUDE_DIR "${2-}" "$deps_dir/spirv-install/include"
emit CMAKE_PROJECT_INCLUDE "${2-}" "$script_dir/llama-static.cmake"
