// Apple runtime smoke test for the dynamically-linked ggml/llama framework.
// dlopen(RTLD_NOW) forces every symbol to bind, which pulls in the entire
// embedded ggml/llama dylib graph — so a successful load proves the graph
// resolves at runtime.
//
// argv[1] = path to the framework binary (…/<name>.framework/<name>)
#include <dlfcn.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: %s <framework-binary>\n", argv[0]); return 2; }
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { fprintf(stderr, "DLOPEN FAILED: %s\n", dlerror()); return 1; }
    void *sym = dlsym(h, "ffi_nobodywho_uniffi_rustbuffer_alloc");
    printf("DLOPEN OK; uniffi symbol %s\n", sym ? "FOUND" : "NOT FOUND (lib loaded)");
    return 0;
}
