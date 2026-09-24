// Android's public OpenCL library owns its ABI. Never inspect vendor objects.
#define CL_TARGET_OPENCL_VERSION 300
#define CL_USE_DEPRECATED_OPENCL_1_2_APIS
#include <CL/cl.h>
#include <CL/cl_ext.h>
#include <dlfcn.h>
#include <pthread.h>
#include <android/log.h>
#include <stdbool.h>
#include <stddef.h>

// Straight to logcat, whatever each binding does with its own logs.
#define LOG(priority, ...) __android_log_print(priority, "nobodywho", __VA_ARGS__)

static pthread_once_t load_once = PTHREAD_ONCE_INIT;
static bool driver_ready;
#define X(required, kind, result, name, params, args) static __typeof__(&name) real_##name;
#include "opencl-functions.inc"
#undef X

static void missing_symbol(const char *name, bool required) {
    LOG(required ? ANDROID_LOG_WARN : ANDROID_LOG_INFO, "OpenCL shim: missing %s (%s)",
        name, required ? "required" : "optional");
    if (required) driver_ready = false;
}

static void load_driver(void) {
    // Intentionally keep the library resident: returned objects depend on it.
    void *driver = dlopen("libOpenCL.so", RTLD_NOW | RTLD_LOCAL);
    if (!driver) {
        LOG(ANDROID_LOG_WARN, "OpenCL shim: cannot load libOpenCL.so: %s; OpenCL unavailable", dlerror());
        return;
    }
    driver_ready = true;
#define X(required, kind, result, name, params, args) \
    real_##name = (__typeof__(&name))dlsym(driver, #name); \
    if (!real_##name) missing_symbol(#name, required);
#include "opencl-functions.inc"
#undef X
    LOG(driver_ready ? ANDROID_LOG_INFO : ANDROID_LOG_WARN, "OpenCL shim: libOpenCL.so %s",
        driver_ready ? "ready" : "unusable; OpenCL unavailable");
}

// Missing optional APIs return errors, never fake success. ggml handles these
// two newer APIs as optional; missing required APIs disables platform discovery.
#define FAIL_INT return CL_INVALID_OPERATION
#define FAIL_HANDLE do { if (errcode_ret) *errcode_ret = CL_INVALID_OPERATION; return NULL; } while (0)
#define FAIL_PLATFORM do { if (count) *count = 0; return CL_PLATFORM_NOT_FOUND_KHR; } while (0)
#define X(required, kind, result, name, params, args) \
    __attribute__((visibility("hidden"))) CL_API_ENTRY result CL_API_CALL name params { \
        pthread_once(&load_once, load_driver); \
        if (!driver_ready || !real_##name) { FAIL_##kind; } \
        return real_##name args; \
    }
#include "opencl-functions.inc"
#undef X
#undef FAIL_PLATFORM
#undef FAIL_HANDLE
#undef FAIL_INT
#undef LOG
