# Static Android GPU backends

Each binding embeds llama.cpp, GGML's CPU/Vulkan/OpenCL backends, the Khronos
OpenCL ICD loader, and the C++ runtime into its native `.so`. Vulkan uses
Android's public `libvulkan.so`. The ICD loader discovers the vendor OpenCL
implementation at runtime; the driver is not required to load NobodyWho.
The existing x86_64 ONNX Runtime companion library is still required.

We build one baseline CPU implementation per ABI (`arm64-v8a`, `x86_64`), not
separate optimized CPU variants. Existing binding package formats are unchanged.

## Build locally

Install CMake, a C/C++ compiler, curl, patch, and Android NDK r28. The helper
uses the NDK's host `glslc`; `VULKAN_GLSLC` can override it. From `nobodywho/`,
in Bash:

```bash
export ANDROID_NDK=/absolute/path/to/android-ndk
gpu_env=$(bash android/prepare-gpu.sh aarch64-linux-android) || exit
eval "$gpu_env"
export ORT_CXX_STDLIB=c++_static
cargo ndk -t arm64-v8a -p 28 build -p nobodywho-uniffi --release --locked
```

Alternatively configure Rust's Android linker / CC / CXX / AR as in build.yml.
The helper downloads immutable Khronos revisions into `target/android-gpu`,
builds a PIC `libOpenCL.a`, and prints shell exports. `--github-env` prints
assignments for GitHub Actions. Explicit header/archive paths avoid modifying
the NDK installation. `core/build.rs` links the loader into the Rust output;
CMake linking alone does not propagate it to Cargo's final link.

## OpenCL discovery

The ICD loader first uses normal `.icd` discovery. An Android-only patch tries
`libOpenCL.so` if no platform was discovered and neither `OCL_ICD_FILENAMES`
nor `OCL_ICD_VENDORS` was supplied. This supports vendors that expose a public
ICD without readable `.icd` files. No driver is packaged. Loader symbols are
hidden so they cannot interpose calls inside the vendor implementation.

Flutter, Kotlin, and React Native library manifests declare:

```xml
<application>
    <uses-native-library android:name="libOpenCL.so" android:required="false" />
</application>
```

Godot Android exports must include this optional declaration in their custom
Android build manifest to access vendor OpenCL on Android 12+. Without it,
Vulkan/CPU fallback remains available. The vendor must expose an ICD-compatible
library accessible to the app; private driver paths are not bypassed.

## Runtime selection

With the existing `use_gpu` / `useGpu` flag enabled, Android selects one device:
OpenCL first, then Vulkan, then CPU. This is a deterministic preference, not a
claim that OpenCL is fastest on every device. The model, draft model, and memory
planner use the same device. Android vision/audio projection stays on CPU
because the current mtmd Rust API cannot select a specific GPU.

`useGpu=false` selects an empty GPU device list and zero GPU layers. Selection
occurs at model load. An unavailable driver is recoverable; a native
driver crash is not. Configure ICD environment overrides before the first
enumeration, which the loader caches for the lifetime of the process.

## CI validation

`build.yml` inspects the linked `.so` for unwanted shared dependencies and
unresolved/exported OpenCL API symbols for both ABIs. Firebase's existing
Pixel 8 (Mali) and Galaxy S24 Ultra (Adreno) jobs test the packaged bindings.
Kotlin source tests repeat in a fresh instrumentation process with both ICD
filenames and the discovery directory pointing to nonexistent entries. That
run must still load the binding and complete inference through Vulkan/CPU.
Both runs also exercise explicit CPU inference. Released-package tests retain
their normal smoke-test coverage. Inspect the native backend/offload logs to
confirm GPU use: successful inference alone can also mean CPU fallback.
