# Android OpenCL linking and packaging

The [upstream llama.cpp instructions][llama-opencl] build Khronos's shared
`libOpenCL.so` and copy it into the NDK sysroot. That file exists so the build
can resolve OpenCL symbols and record this runtime dependency:

```text
DT_NEEDED: libOpenCL.so
```

`BUILD_SHARED_LIBS=OFF` only makes llama.cpp and GGML static. It does not make
the separately built OpenCL loader static.

## The NDK is not the APK

The NDK sysroot is compiler input on the build machine. `APK/lib/arm64-v8a/`
contains libraries shipped to the device. Copying a library into the first
location does not copy it into the second.

Upstream's documented command-line build does not package the Khronos loader.
Android therefore satisfies `DT_NEEDED` with the device's public library:

```text
llama.cpp -> device libOpenCL.so -> vendor implementation
```

If an app deliberately packages the Khronos loader, the path changes to:

```text
llama.cpp -> packaged Khronos loader -> discovered vendor ICD -> vendor implementation
```

That adds ICD discovery and Android linker-namespace concerns. It is not the
same as calling the device library directly.

## How NobodyWho calls OpenCL

A hard OpenCL dependency can stop NobodyWho itself from loading on devices
without OpenCL, before Vulkan or CPU fallback can run. NobodyWho links no
OpenCL library at all:

1. `llama-static.cmake` force-includes [`opencl-api.h`](opencl-api.h) into
   ggml-opencl. It turns off the OpenCL prototypes and declares every function
   ggml calls as a pointer, typed by the real headers.
2. On the first `clGetPlatformIDs`, [`core/src/opencl.rs`](../core/src/opencl.rs)
   calls `dlopen("libOpenCL.so")` and fills each pointer by name with `dlsym`.
3. If the library or a required function is absent, ggml sees no platforms.

```text
ggml -> function pointers -> device libOpenCL.so -> vendor implementation
```

Nothing interprets vendor ICD dispatch tables. This avoids the dispatch layout
mismatch seen with the tested Qualcomm driver while keeping OpenCL optional.
When ggml starts calling a new OpenCL function, its compile fails with an
undeclared identifier; add the name to
[`opencl-functions.inc`](opencl-functions.inc).

## Verify the result

```bash
readelf -d libnobodywho.so | grep NEEDED       # link-time dependencies
unzip -l app.apk | grep '/libOpenCL\.so$'      # libraries packaged in the APK
adb shell "cat /proc/$(pidof your.package)/maps | grep -i OpenCL" # runtime
```

[llama-opencl]: https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/OPENCL.md
