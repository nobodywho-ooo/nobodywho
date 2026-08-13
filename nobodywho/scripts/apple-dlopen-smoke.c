#include <dlfcn.h>
#include <libgen.h>
#include <limits.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <framework-binary>\n", argv[0]);
        return 2;
    }
    if (!dlopen(argv[1], RTLD_NOW | RTLD_LOCAL)) {
        fprintf(stderr, "framework load failed: %s\n", dlerror());
        return 1;
    }

    char path[PATH_MAX];
    snprintf(path, sizeof(path), "%s/libggml.dylib", dirname(argv[1]));
    void *ggml = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!ggml) {
        fprintf(stderr, "ggml load failed: %s\n", dlerror());
        return 1;
    }

    void (*load)(const char *) = dlsym(ggml, "ggml_backend_load_all_from_path");
    size_t (*device_count)(void) = dlsym(ggml, "ggml_backend_dev_count");
    if (!load || !device_count) {
        fputs("ggml loader symbols missing\n", stderr);
        return 1;
    }

    char *directory = dirname(path);
    load(directory);
    size_t devices = device_count();
    printf("SMOKE OK: %zu backend device(s)\n", devices);
    return devices == 0;
}
