# Android OpenCL linking and packaging

This note explains how the upstream llama.cpp Android build finds OpenCL and
why placing `libOpenCL.so` in the Android NDK is not the same as shipping that
library in an APK.

## How the upstream build works

The [llama.cpp OpenCL instructions][llama-opencl] build Khronos's
OpenCL ICD loader as a shared `libOpenCL.so`, then copy it into the NDK
sysroot before building llama.cpp.

The copy in the NDK gives the compiler and linker:

- the OpenCL symbols needed to link llama.cpp; and
- the library metadata needed to record a runtime dependency.

The resulting native executable or shared library normally contains:

```text
DT_NEEDED: libOpenCL.so
```

`BUILD_SHARED_LIBS=OFF` makes llama.cpp and GGML libraries static. It does not
make the separately built OpenCL library static.

For the standalone command-line build described by upstream, the Khronos
library is used while linking but is not copied to the device by the documented
commands. At runtime, Android resolves `libOpenCL.so` from the device, such as
Qualcomm's public `/vendor/lib64/libOpenCL.so`.

```text
llama.cpp
    -> device libOpenCL.so
    -> vendor OpenCL implementation
```

This matches Khronos's [Android ICD installation model][khronos-install]: an
SDK or NDK supplies a library for linking, while the target device supplies the
runtime OpenCL library.

## NDK sysroot versus APK packaging

These locations have different purposes:

```text
NDK sysroot/usr/lib/.../libOpenCL.so
    Used by the linker on the build machine.

APK/lib/arm64-v8a/libOpenCL.so
    Shipped with the application and available at runtime.
```

Copying `libOpenCL.so` into the NDK does not automatically mean it will be
included in the APK. APK contents are determined separately by the Android
packaging configuration.

If the Khronos loader is deliberately packaged in the APK, Android will
normally use that copy to satisfy the `DT_NEEDED` dependency. The runtime path
then becomes:

```text
llama.cpp
    -> packaged Khronos libOpenCL.so
    -> vendor ICD discovered by the Khronos loader
    -> vendor OpenCL implementation
```

That requires the Khronos loader to discover and load the vendor ICD. On
Android it normally searches a vendor ICD configuration directory, or uses
configuration such as `OCL_ICD_FILENAMES`. This is not equivalent to calling
the device's public `libOpenCL.so` directly, and it can expose compatibility or
Android linker-namespace problems.

## Why NobodyWho uses a forwarding shim

NobodyWho wants OpenCL to remain optional. A hard `DT_NEEDED` dependency can
prevent the entire NobodyWho native library from loading on a device that does
not provide `libOpenCL.so`, before the library has an opportunity to fall back
to Vulkan or CPU.

The forwarding shim therefore:

1. keeps the NobodyWho native library free of a hard OpenCL dependency;
2. calls `dlopen("libOpenCL.so")` at runtime;
3. resolves OpenCL entry points by name with `dlsym`; and
4. reports OpenCL as unavailable when the device library cannot be loaded.

```text
NobodyWho
    -> forwarding shim
    -> dlopen(device libOpenCL.so)
    -> vendor OpenCL implementation
```

The shim does not interpret vendor objects or their ICD dispatch tables. This
is important because the previously embedded Khronos ICD loader encountered a
dispatch-table incompatibility with the tested Qualcomm implementation.

## Verifying an actual build

Use all three checks because they answer different questions:

```bash
# Does the native library have a hard OpenCL dependency?
readelf -d libnobodywho.so | grep NEEDED

# Does the APK contain its own OpenCL library?
unzip -l app.apk | grep '/libOpenCL\.so$'

# Which OpenCL libraries did Android actually load?
adb shell "cat /proc/$(pidof your.package)/maps | grep -i OpenCL"
```

The first check describes linking, the second describes packaging, and the
third establishes the runtime result.

[llama-opencl]: https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/OPENCL.md
[khronos-install]: https://github.com/KhronosGroup/OpenCL-Docs/blob/main/OpenCL_ICD_Installation.txt
