# Injected into llama.cpp's CMake configure via CMAKE_PROJECT_INCLUDE (see
# nobodywho/.cargo/config.toml). Runs as the last step of each project() call —
# i.e. before llama.cpp's `option(LLAMA_OPENSSL ... ON)` — so force-setting the
# cache entry here makes that option() keep OFF.
#
# Why: with the `dynamic-link` feature llama.cpp's `common` helper is built as a
# SHARED library, which forces the linker to resolve cpp-httplib's TLS symbols.
# llama.cpp defaults LLAMA_OPENSSL=ON and finds the *host* OpenSSL at configure
# time, but the x86_64-apple-darwin CROSS build then has no x86_64 OpenSSL to link
# against -> undefined _SSL_*/_X509_*/_EVP_* symbols. We don't use llama.cpp's
# HTTPS downloader (LLAMA_CURL is already OFF; model I/O is our own), so disabling
# httplib TLS is safe and makes the shared `common` lib link on every arch.
set(LLAMA_OPENSSL OFF CACHE BOOL "nobodywho: httplib TLS unused; breaks shared common cross-link" FORCE)
