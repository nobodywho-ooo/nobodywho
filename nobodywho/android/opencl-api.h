// Force-included into ggml-opencl by llama-static.cmake. Replaces the OpenCL
// prototypes with pointers that core/src/opencl.rs fills by name from the
// device's libOpenCL.so, so ggml never links against OpenCL. The asm labels
// give the pointers namespaced ELF symbols; without this header, ggml's calls
// would reference plain cl* symbols that nothing defines, so the link fails.
#pragma once

// The same settings ggml-opencl makes before its own #include <CL/cl.h>.
#define CL_TARGET_OPENCL_VERSION GGML_OPENCL_TARGET_VERSION
#define CL_USE_DEPRECATED_OPENCL_1_2_APIS
#define CL_NO_PROTOTYPES
#include <CL/cl.h>
#include <CL/cl_function_types.h>

#ifdef __cplusplus
extern "C" {
#endif

// A real function: loads the driver on first use. ggml calls it first.
extern clGetPlatformIDs_t clGetPlatformIDs __asm__("nobodywho_clGetPlatformIDs");

#define X(name) extern name##_fn name __asm__("nobodywho_" #name);
#include "opencl-functions.inc"
#undef X

#ifdef __cplusplus
}
#endif
