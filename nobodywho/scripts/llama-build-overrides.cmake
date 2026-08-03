# Injected into llama.cpp's CMake configure via CMAKE_PROJECT_INCLUDE (see
# nobodywho/.cargo/config.toml). Overrides needed by the dynamic-link / dynamic-backends build:

# 1) Disable OpenSSL in cpp-httplib. dynamic-link builds `common` as a shared lib,
#    forcing its TLS symbols to resolve, but the x86_64-apple-darwin cross build has
#    no host OpenSSL to link. We don't use the HTTPS downloader.
set(LLAMA_OPENSSL OFF CACHE BOOL "nobodywho: httplib TLS unused; breaks shared common cross-link" FORCE)

# 1b) GGML_CPU_ALL_VARIANTS (from the dynamic-backends feature) builds every CPU
#     microarchitecture variant itself, and ggml rejects combining it with a fixed
#     GGML_CPU_ARM_ARCH ("Cannot use both ..."). llama-cpp-sys-2's build.rs sets
#     GGML_CPU_ARM_ARCH=armv8-a for aarch64 targets, so clear it here and let
#     ALL_VARIANTS own ARM variant selection.
if(GGML_CPU_ALL_VARIANTS AND GGML_CPU_ARM_ARCH)
  set(GGML_CPU_ARM_ARCH "" CACHE STRING "nobodywho: cleared — ALL_VARIANTS owns ARM variants" FORCE)
endif()

# 1c) GGML_CPU_ALL_VARIANTS compiles ggml's per-variant cpu-feats.cpp, which on Apple
#     includes <sys/sysctl.h>. Its BSD types (u_int, u_char, …) are hidden when
#     _POSIX_C_SOURCE is defined without _DARWIN_C_SOURCE — the case on the visionOS/watchOS
#     toolchains (not the macOS default), giving "unknown type name 'u_int'". Define it so the
#     feature-detection TU compiles on every Apple slice; sysctlbyname itself is available on all.
if(APPLE)
  set(CMAKE_C_FLAGS   "${CMAKE_C_FLAGS} -D_DARWIN_C_SOURCE"   CACHE STRING "" FORCE)
  set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -D_DARWIN_C_SOURCE" CACHE STRING "" FORCE)
endif()

# 2) For every ggml/llama shared lib: strip the versioned SONAME (so it's born as
#    plain libggml.so/.dylib, matching what packaging references) and give it an
#    $ORIGIN (ELF) / @loader_path (Mach-O) runpath. DT_RUNPATH doesn't chain, so
#    each lib needs its own runpath to find siblings (libggml -> libggml-base).
#    BUILD_WITH_INSTALL_RPATH bakes that rpath into the shipped build-tree lib.
#    Targets are enumerated dynamically (no hardcoded backend list) and the fixup
#    is deferred via cmake_language(DEFER) so it runs after they exist. CMake >= 3.19.
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
