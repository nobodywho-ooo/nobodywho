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

# List of absolute paths to libraries that should be bundled with the plugin.
set(nobodywho_bundled_libraries
  ${NOBODYWHO_LIB_PATH}
  PARENT_SCOPE
)
