# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

# Project-level configuration.
set(PROJECT_NAME "nobodywho")
project(${PROJECT_NAME} LANGUAGES CXX)

# The binary resolution is handled by a Dart script that supports multiple methods:
# 1. Environment variable override (NOBODYWHO_FLUTTER_LIB_PATH)
# 2. Local cargo build detection
# 3. Cached download
# 4. Download from GitHub releases

# Detect architecture
if(CMAKE_SYSTEM_PROCESSOR MATCHES "aarch64|arm64|ARM64")
  set(LINUX_ARCH "aarch64")
elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64|amd64")
  set(LINUX_ARCH "x86_64")
else()
  message(FATAL_ERROR "Unsupported architecture: ${CMAKE_SYSTEM_PROCESSOR}")
endif()

# Resolve binary using Dart script
execute_process(
  COMMAND dart run "${CMAKE_CURRENT_SOURCE_DIR}/../tool/resolve_binary.dart"
          --platform=linux
          --arch=${LINUX_ARCH}
          --build-type=release
          --cache-dir=${CMAKE_CURRENT_BINARY_DIR}
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/.."
  OUTPUT_VARIABLE NOBODYWHO_LIB_PATH
  OUTPUT_STRIP_TRAILING_WHITESPACE
  ERROR_VARIABLE resolve_error
  RESULT_VARIABLE resolve_result
)

if(NOT resolve_result EQUAL 0)
  message(FATAL_ERROR "Failed to resolve NobodyWho binary for Linux ${LINUX_ARCH}:\n${resolve_error}")
endif()

message(STATUS "Using NobodyWho library for Linux ${LINUX_ARCH}: ${NOBODYWHO_LIB_PATH}")

get_filename_component(NOBODYWHO_LIB_DIR "${NOBODYWHO_LIB_PATH}" DIRECTORY)

# Start with the main library and accumulate siblings into this list.
set(_bundled_libs "${NOBODYWHO_LIB_PATH}")

# Whisper module (optional – STT falls back to runtime discovery if absent).
set(_whisper "${NOBODYWHO_LIB_DIR}/libnobodywho_stt.so")
if(EXISTS "${_whisper}")
  message(STATUS "Bundling STT module: ${_whisper}")
  list(APPEND _bundled_libs "${_whisper}")
else()
  message(STATUS "Whisper module not found at ${_whisper}; STT will use runtime discovery")
endif()

# GGML shared libs – present when built with dynamic-backends / dynamic-link.
# libggml-base and libllama are hard runtime deps; without them the extension won't load.
foreach(_lib libggml-base.so libllama.so)
  if(EXISTS "${NOBODYWHO_LIB_DIR}/${_lib}")
    message(STATUS "Bundling GGML lib: ${NOBODYWHO_LIB_DIR}/${_lib}")
    list(APPEND _bundled_libs "${NOBODYWHO_LIB_DIR}/${_lib}")
  endif()
endforeach()

# GGML backend modules – dlopen'd at runtime for hardware selection.
# Broad glob catches all backends: CPU variants, Vulkan, QNN, OpenCL, RPC, CANN, etc.
file(GLOB _backend_modules
  "${NOBODYWHO_LIB_DIR}/libggml-*.so"
  "${NOBODYWHO_LIB_DIR}/ggml-*.so"
)
# libggml-base.so is a hard dep handled above, not a dlopen'd module.
list(FILTER _backend_modules EXCLUDE REGEX "libggml-base\\.so$")
if(_backend_modules)
  message(STATUS "Bundling GGML backend modules: ${_backend_modules}")
  list(APPEND _bundled_libs ${_backend_modules})
endif()

set(nobodywho_bundled_libraries ${_bundled_libs} PARENT_SCOPE)
