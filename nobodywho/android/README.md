# Android GPU builds

The binding `.so` embeds GGML's CPU/OpenCL/Vulkan backends, a small OpenCL
forwarding shim and C++ runtime. Android supplies `libvulkan.so` and optional OpenCL
drivers. The existing x86_64 ONNX Runtime companion library is still required.

With NDK r28, CMake and curl installed, run from `nobodywho/` in Bash:

```bash
export ANDROID_NDK=/absolute/path/to/android-ndk
gpu_env=$(bash android/prepare-gpu.sh aarch64-linux-android) || exit
eval "$gpu_env"
export ORT_CXX_STDLIB=c++_static
cargo ndk -t arm64-v8a -p 28 build -p nobodywho-uniffi --release --locked
```

The helper caches pinned dependencies in `target/android-gpu`; `--github-env`
prints CI environment assignments. `VULKAN_GLSLC` overrides the NDK compiler.
The shim opens the device's public `libOpenCL.so` once and resolves functions
by name. It never reads vendor objects' ICD dispatch tables. This path is
vendor-neutral, including Qualcomm and Mali; device testing is still required.
Missing libraries or required OpenCL 1.2 entry points disable OpenCL discovery.
The newer buffer-with-properties and subgroup-info APIs are optional.
Flutter/Kotlin/React Native declare it optional; Godot custom Android exports
must add `<uses-native-library android:name="libOpenCL.so" android:required="false" />`
inside `<application>` to access public vendor drivers on Android 12+.

Selection is OpenCL → Vulkan → CPU; `useGpu=false` forces CPU. The current
Adreno Vulkan exclusion avoids observed shader crashes (llama.cpp#12421),
while Turnip remains eligible. Vision/audio projection stays on CPU because
mtmd cannot select its GPU. Selection does not recover from native driver crashes.

See [`OPENCL_LINKING.md`](OPENCL_LINKING.md) for the difference between
link-time and packaged OpenCL libraries.
