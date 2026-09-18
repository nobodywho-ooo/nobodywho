# Android GPU builds

The binding `.so` embeds GGML's CPU/OpenCL/Vulkan backends, the OpenCL ICD
loader and C++ runtime. Android supplies `libvulkan.so` and optional OpenCL
drivers. The existing x86_64 ONNX Runtime companion library is still required.

With NDK r28, CMake, curl and patch installed, run from `nobodywho/` in Bash:

```bash
export ANDROID_NDK=/absolute/path/to/android-ndk
gpu_env=$(bash android/prepare-gpu.sh aarch64-linux-android) || exit
eval "$gpu_env"
export ORT_CXX_STDLIB=c++_static
cargo ndk -t arm64-v8a -p 28 build -p nobodywho-uniffi --release --locked
```

The helper caches pinned dependencies in `target/android-gpu`; `--github-env`
prints CI environment assignments. `VULKAN_GLSLC` overrides the NDK compiler.
The loader patch tries `libOpenCL.so` when `.icd` discovery finds no vendors,
unless `OCL_ICD_FILENAMES` or `OCL_ICD_VENDORS` explicitly overrides discovery.
It also accepts a `libOpenCL.so` that is itself an ICD loader (Qualcomm's wraps
`libOpenCL_adreno.so`, which apps cannot load): such a library lacks
`clIcdGetPlatformIDsKHR`, so the patch enumerates it through `clGetPlatformIDs`.
The platforms it returns are the ICD's own objects and dispatch normally.
Flutter/Kotlin/React Native declare it optional; Godot custom Android exports
must add `<uses-native-library android:name="libOpenCL.so" android:required="false" />`
inside `<application>` to access public vendor drivers on Android 12+.

Selection is OpenCL → Vulkan → CPU; `useGpu=false` forces CPU. The current
Adreno Vulkan exclusion avoids observed shader crashes (llama.cpp#12421),
while Turnip remains eligible. Vision/audio projection stays on CPU because
mtmd cannot select its GPU. Selection does not recover from native driver crashes.

CI checks shared dependencies and runs Firebase inference with normal and
missing OpenCL discovery, plus explicit CPU mode. Unit tests cover selection
order and the Adreno exclusion. Flutter/Kotlin forward native diagnostics;
ICD tracing defaults on unless the host sets `OCL_ICD_ENABLE_TRACE` itself.
