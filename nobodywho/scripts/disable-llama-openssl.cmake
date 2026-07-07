# Injected via CMAKE_PROJECT_INCLUDE (see nobodywho/.cargo/config.toml); runs before
# llama.cpp's option(LLAMA_OPENSSL ... ON), so this FORCE keeps it OFF. Rationale in
# .cargo/config.toml: dynamic-link's shared `common` forces httplib TLS symbols the
# x86_64-apple-darwin cross build can't link, and we don't use the HTTPS downloader.
set(LLAMA_OPENSSL OFF CACHE BOOL "nobodywho: httplib TLS unused; breaks shared common cross-link" FORCE)
