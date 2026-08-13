# The Flutter tooling requires that developers have a version of Visual Studio
# installed that includes CMake 3.14 or later. You should not increase this
# version, as doing so will cause the plugin to fail to compile for some
# customers of the plugin.
cmake_minimum_required(VERSION 3.14)

set(PROJECT_NAME "nobodywho")
project(${PROJECT_NAME} LANGUAGES CXX)

set(WINDOWS_ARCH "x86_64")

if(DEFINED ENV{FLUTTER_ROOT})
  set(DART_EXECUTABLE "$ENV{FLUTTER_ROOT}/bin/cache/dart-sdk/bin/dart.exe")
else()
  find_program(DART_EXECUTABLE NAMES dart.exe dart)
  if(NOT DART_EXECUTABLE)
    message(FATAL_ERROR "Could not find 'dart' executable. Ensure Flutter is in your PATH or FLUTTER_ROOT is set.")
  endif()
endif()

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
file(GLOB NOBODYWHO_RUNTIME_LIBS
  "${NOBODYWHO_LIB_DIR}/nobodywho-runtime/*.dll"
)

set(nobodywho_bundled_libraries
  "${NOBODYWHO_LIB_PATH}"
  ${NOBODYWHO_RUNTIME_LIBS}
  PARENT_SCOPE
)
