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

# List of absolute paths to libraries that should be bundled with the plugin.
set(nobodywho_bundled_libraries
  ${NOBODYWHO_LIB_PATH}
  PARENT_SCOPE
)
