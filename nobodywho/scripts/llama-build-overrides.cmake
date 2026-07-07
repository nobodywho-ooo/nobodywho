# Injected into llama.cpp's CMake configure via CMAKE_PROJECT_INCLUDE (see
# nobodywho/.cargo/config.toml). Two overrides needed by the dynamic-link build:

# 1) Disable OpenSSL in cpp-httplib. dynamic-link builds `common` as a shared lib,
#    forcing its TLS symbols to resolve, but the x86_64-apple-darwin cross build has
#    no host OpenSSL to link. We don't use the HTTPS downloader. This runs before
#    llama.cpp's option(LLAMA_OPENSSL ... ON), so the FORCE keeps it OFF.
set(LLAMA_OPENSSL OFF CACHE BOOL "nobodywho: httplib TLS unused; breaks shared common cross-link" FORCE)

# 2) Strip the versioned SONAME / dylib version from every ggml/llama shared lib so
#    they are born as plain libggml.so / libggml.dylib (matching the SONAME the
#    consuming cdylib records and the unversioned names our packaging references),
#    instead of libggml.so.0 / libggml.0.dylib. Enumerated dynamically so it covers
#    every backend (ggml-cpu, ggml-vulkan, ggml-metal, ...) without a hardcoded list,
#    and deferred so it runs after the targets are created.
#    Requires CMake >= 3.19 (cmake_language DEFER).
get_property(_nw_hooked GLOBAL PROPERTY _NW_SOVERSION_HOOKED)
if(NOT _nw_hooked)
  set_property(GLOBAL PROPERTY _NW_SOVERSION_HOOKED ON)
  function(_nw_strip_soversion_dir dir)
    get_property(_tgts DIRECTORY "${dir}" PROPERTY BUILDSYSTEM_TARGETS)
    foreach(_t IN LISTS _tgts)
      if("${_t}" MATCHES "^(ggml|llama|mtmd)")
        get_target_property(_type ${_t} TYPE)
        if(_type STREQUAL "SHARED_LIBRARY")
          set_property(TARGET ${_t} PROPERTY VERSION)
          set_property(TARGET ${_t} PROPERTY SOVERSION)
        endif()
      endif()
    endforeach()
    get_property(_subs DIRECTORY "${dir}" PROPERTY SUBDIRECTORIES)
    foreach(_s IN LISTS _subs)
      _nw_strip_soversion_dir("${_s}")
    endforeach()
  endfunction()
  function(_nw_strip_soversion)
    _nw_strip_soversion_dir("${CMAKE_SOURCE_DIR}")
  endfunction()
  cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nw_strip_soversion)
endif()
