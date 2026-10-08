# Android GPU builds

The binding `.so` embeds GGML's CPU/OpenCL/Vulkan backends, a small OpenCL
forwarding shim and C++ runtime. Android supplies `libvulkan.so` and optional OpenCL
drivers. The existing x86_64 ONNX Runtime companion library is still required.

With NDK r28, CMake and curl installed, run from `nobodywho/` in Bash:

```bash
export ANDROID_NDK_ROOT=/absolute/path/to/android-ndk
gpu_env=$(./android/prepare-gpu.sh aarch64-linux-android) || exit
eval "$gpu_env"
export ORT_CXX_STDLIB=c++_static
cargo ndk -t arm64-v8a -p 28 build -p nobodywho-uniffi --release --locked
```

The helper caches pinned dependencies in `target/android-gpu`; `--github-env`
prints CI environment assignments. `VULKAN_GLSLC` overrides the NDK compiler.


## Shim

Ideally, we'd use `OpenCL-ICD-Loader`, but that doesn't really seem to support
static linking, see:
<https://github.com/KhronosGroup/OpenCL-ICD-Loader/blob/5192c84f8059e5f703e5452929b613f9487f6e4c/CMakeLists.txt#L16-L49>

FIXME(madsmtm): Maybe report and fix this upstream in OpenCL-ICD-Loader?

Instead, we create a small shim that `dlopen`s the device's public
`libOpenCL.so` and resolves functions using `dlsym`, instead of reading vendor
objects' ICD dispatch tables.

Note that a `<uses-native-library android:name="libOpenCL.so" android:required="false" />`
inside `<application>` is required to access public vendor drivers on Android 12+.
