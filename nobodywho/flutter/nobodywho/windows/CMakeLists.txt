# The Flutter tooling requires that developers have a version of Visual Studio
# installed that includes CMake 3.14 or later. You should not increase this
# version, as doing so will cause the plugin to fail to compile for some
# customers of the plugin.
cmake_minimum_required(VERSION 3.14)

# Project-level configuration.
set(PROJECT_NAME "nobodywho")
project(${PROJECT_NAME} LANGUAGES CXX)

# The binary resolution is handled by a Dart script that supports multiple methods:
# 1. Environment variable override (NOBODYWHO_FLUTTER_LIB_PATH)
# 2. Local cargo build detection
# 3. Cached download
# 4. Download from GitHub releases

# Windows only supports x86_64 currently
set(WINDOWS_ARCH "x86_64")

# Find dart executable - Flutter sets FLUTTER_ROOT during builds
# On Windows, bin/dart is a .bat file; the real executable is in the bundled dart-sdk
if(DEFINED ENV{FLUTTER_ROOT})
  set(DART_EXECUTABLE "$ENV{FLUTTER_ROOT}/bin/cache/dart-sdk/bin/dart.exe")
else()
  find_program(DART_EXECUTABLE NAMES dart.exe dart)
  if(NOT DART_EXECUTABLE)
    message(FATAL_ERROR "Could not find 'dart' executable. Ensure Flutter is in your PATH or FLUTTER_ROOT is set.")
  endif()
endif()

# Resolve binary using Dart script
execute_process(
  COMMAND ${DART_EXECUTABLE} run "${CMAKE_CURRENT_SOURCE_DIR}/../tool/resolve_binary.dart"
          --platform=windows
          --arch=${WINDOWS_ARCH}
          --build-type=release
          --cache-dir=${CMAKE_CURRENT_BINARY_DIR}
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/.."
  OUTPUT_VARIABLE NOBODYWHO_LIB_PATH
  OUTPUT_STRIP_TRAILING_WHITESPACE
  ERROR_VARIABLE resolve_error
  RESULT_VARIABLE resolve_result
)

if(NOT resolve_result EQUAL 0)
  message(FATAL_ERROR "Failed to resolve NobodyWho binary for Windows ${WINDOWS_ARCH}:\n${resolve_error}")
endif()

message(STATUS "Using NobodyWho library for Windows ${WINDOWS_ARCH}: ${NOBODYWHO_LIB_PATH}")

get_filename_component(NOBODYWHO_LIB_DIR "${NOBODYWHO_LIB_PATH}" DIRECTORY)

# Start with the main library and accumulate siblings into this list.
set(_bundled_libs "${NOBODYWHO_LIB_PATH}")

# Whisper module (optional – STT falls back to runtime discovery if absent).
set(_whisper "${NOBODYWHO_LIB_DIR}/nobodywho_stt.dll")
if(EXISTS "${_whisper}")
  message(STATUS "Bundling STT module: ${_whisper}")
  list(APPEND _bundled_libs "${_whisper}")
else()
  message(STATUS "Whisper module not found at ${_whisper}; STT will use runtime discovery")
endif()

# GGML shared libs – present when built with dynamic-backends / dynamic-link.
foreach(_lib ggml-base.dll llama.dll)
  if(EXISTS "${NOBODYWHO_LIB_DIR}/${_lib}")
    message(STATUS "Bundling GGML lib: ${NOBODYWHO_LIB_DIR}/${_lib}")
    list(APPEND _bundled_libs "${NOBODYWHO_LIB_DIR}/${_lib}")
  endif()
endforeach()

# GGML backend modules – dlopen'd at runtime for hardware selection.
# Broad glob catches all backends: CPU variants, Vulkan, QNN, OpenCL, RPC, CANN, etc.
file(GLOB _backend_modules
  "${NOBODYWHO_LIB_DIR}/ggml-*.dll"
)
list(FILTER _backend_modules EXCLUDE REGEX "ggml-base\\.dll$")
if(_backend_modules)
  message(STATUS "Bundling GGML backend modules: ${_backend_modules}")
  list(APPEND _bundled_libs ${_backend_modules})
endif()

set(nobodywho_bundled_libraries ${_bundled_libs} PARENT_SCOPE)
