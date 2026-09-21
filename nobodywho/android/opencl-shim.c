// Android's public OpenCL library owns its ABI. Never inspect vendor objects.
#define CL_TARGET_OPENCL_VERSION 300
#define CL_USE_DEPRECATED_OPENCL_1_2_APIS
#include <CL/cl.h>
#include <CL/cl_ext.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

static pthread_once_t once = PTHREAD_ONCE_INIT;
static bool available;
#define X(required, kind, result, name, params, args) static __typeof__(&name) p_##name;
#include "opencl-functions.inc"
#undef X

static void load_driver(void) {
    const char *path = getenv("NOBODYWHO_OPENCL_LIBRARY");
    if (!path || !*path) path = "libOpenCL.so";
    // Intentionally keep the library resident: returned objects depend on it.
    void *driver = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!driver) {
        fprintf(stderr, "OpenCL shim: cannot load %s: %s; OpenCL unavailable\n", path, dlerror());
        return;
    }
    available = true;
#define X(required, kind, result, name, params, args) \
    p_##name = (__typeof__(&name))dlsym(driver, #name); \
    if (!p_##name) { \
        fprintf(stderr, "OpenCL shim: missing %s (%s)\n", #name, required ? "required" : "optional"); \
        if (required) available = false; \
    }
#include "opencl-functions.inc"
#undef X
    fprintf(stderr, "OpenCL shim: %s %s\n", path, available ? "ready" : "unusable; OpenCL unavailable");
}

// Missing optional APIs return errors, never fake success. ggml handles these
// two newer APIs as optional; missing required APIs disables platform discovery.
#define FAIL_INT return CL_INVALID_OPERATION
#define FAIL_HANDLE do { if (errcode_ret) *errcode_ret = CL_INVALID_OPERATION; return NULL; } while (0)
#define FAIL_PLATFORM do { if (count) *count = 0; return CL_PLATFORM_NOT_FOUND_KHR; } while (0)
#define X(required, kind, result, name, params, args) \
    __attribute__((visibility("hidden"))) CL_API_ENTRY result CL_API_CALL name params { \
        pthread_once(&once, load_driver); \
        if (!available || !p_##name) { FAIL_##kind; } \
        return p_##name args; \
    }
#include "opencl-functions.inc"
