# Injected into llama.cpp's CMake configure via CMAKE_PROJECT_INCLUDE (see
# nobodywho/.cargo/config.toml). Two overrides needed by the dynamic-link build:

# 1) Disable OpenSSL in cpp-httplib. dynamic-link builds `common` as a shared lib,
#    forcing its TLS symbols to resolve, but the x86_64-apple-darwin cross build has
#    no host OpenSSL to link. We don't use the HTTPS downloader. This runs before
#    llama.cpp's option(LLAMA_OPENSSL ... ON), so the FORCE keeps it OFF.
set(LLAMA_OPENSSL OFF CACHE BOOL "nobodywho: httplib TLS unused; breaks shared common cross-link" FORCE)

# 2) Fix up every ggml/llama shared lib so it ships correctly:
#    - strip the versioned SONAME/dylib version -> born as plain libggml.so /
#      libggml.dylib (matching the SONAME the consuming cdylib records and the
#      unversioned names our packaging references), not libggml.so.0 / libggml.0.dylib;
#    - give each lib an $ORIGIN (ELF) / @loader_path (Mach-O) runpath so a co-located
#      consumer resolves the transitive graph (DT_RUNPATH does NOT chain, so each lib
#      needs its own to find its siblings, e.g. libggml -> libggml-base).
#    BUILD_WITH_INSTALL_RPATH makes the build-tree lib (the one shipped) carry exactly
#    INSTALL_RPATH, with no automatic build-tree absolute paths.
#    Enumerated dynamically to cover every backend (ggml-cpu, ggml-vulkan, ...) without
#    a hardcoded list, and deferred so it runs after the targets are created.
#    Requires CMake >= 3.19 (cmake_language DEFER).
get_property(_nw_hooked GLOBAL PROPERTY _NW_GGML_FIXUP_HOOKED)
if(NOT _nw_hooked)
  set_property(GLOBAL PROPERTY _NW_GGML_FIXUP_HOOKED ON)
  function(_nw_fixup_ggml_libs_dir dir)
    if(APPLE)
      set(_rp "@loader_path")
    else()
      set(_rp "$ORIGIN")
    endif()
    get_property(_tgts DIRECTORY "${dir}" PROPERTY BUILDSYSTEM_TARGETS)
    foreach(_t IN LISTS _tgts)
      if("${_t}" MATCHES "^(ggml|llama|mtmd)")
        get_target_property(_type ${_t} TYPE)
        if(_type STREQUAL "SHARED_LIBRARY")
          set_property(TARGET ${_t} PROPERTY VERSION)
          set_property(TARGET ${_t} PROPERTY SOVERSION)
          set_target_properties(${_t} PROPERTIES
            INSTALL_RPATH "${_rp}"
            BUILD_WITH_INSTALL_RPATH ON)
        endif()
      endif()
    endforeach()
    get_property(_subs DIRECTORY "${dir}" PROPERTY SUBDIRECTORIES)
    foreach(_s IN LISTS _subs)
      _nw_fixup_ggml_libs_dir("${_s}")
    endforeach()
  endfunction()
  function(_nw_fixup_ggml_libs)
    _nw_fixup_ggml_libs_dir("${CMAKE_SOURCE_DIR}")
  endfunction()
  cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nw_fixup_ggml_libs)
endif()
