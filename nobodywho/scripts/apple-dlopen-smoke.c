// Apple runtime smoke test for the dynamically-linked ggml/llama framework.
//
// 1. dlopen(RTLD_NOW) forces every symbol to bind, pulling in the embedded ggml/llama
//    dylib graph — a successful load proves that @rpath/@loader_path graph resolves.
// 2. Then it loads the dlopen'd backend MODULES (libggml-cpu-*.so, libggml-metal.so —
//    GGML_BACKEND_DL) from the framework dir the way core/llm.rs does at runtime, and
//    asserts at least one backend device registered. That proves the CPU-variant / Metal
//    modules are embedded, signed acceptably for the simulator, and actually loadable.
//
// argv[1] = path to the framework binary (…/<name>.framework/<name>)
#include <dlfcn.h>
#include <libgen.h>
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: %s <framework-binary>\n", argv[0]); return 2; }

    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { fprintf(stderr, "DLOPEN FAILED: %s\n", dlerror()); return 1; }
    void *sym = dlsym(h, "ffi_nobodywho_uniffi_rustbuffer_alloc");
    printf("DLOPEN OK; uniffi symbol %s\n", sym ? "FOUND" : "NOT FOUND (lib loaded)");

    // Load the backend modules from the framework dir (dirname of argv[1]) via libggml's
    // public loader, then count registered devices.
    char buf[4096];
    strncpy(buf, argv[1], sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = 0;
    const char *dir = dirname(buf);

    char ggml_path[4096];
    snprintf(ggml_path, sizeof(ggml_path), "%s/libggml.dylib", dir);
    void *g = dlopen(ggml_path, RTLD_NOW | RTLD_LOCAL);
    if (!g) { fprintf(stderr, "BACKENDS FAILED: dlopen %s: %s\n", ggml_path, dlerror()); return 1; }

    void (*load_all)(const char *) = (void (*)(const char *)) dlsym(g, "ggml_backend_load_all_from_path");
    size_t (*dev_count)(void) = (size_t (*)(void)) dlsym(g, "ggml_backend_dev_count");
    if (!load_all || !dev_count) { fprintf(stderr, "BACKENDS FAILED: ggml loader symbols missing\n"); return 1; }

    load_all(dir);
    size_t n = dev_count();
    if (n == 0) { fprintf(stderr, "BACKENDS FAILED: no devices registered — modules did not load\n"); return 1; }

    printf("SMOKE OK: %zu backend device(s) registered from %s\n", n, dir);
    return 0;
}
