set(PROJECT_NAME "nobodywho")

# Unlike the Windows & Linux CMakeLists.txt, this Android equivalent is just here
# to download the Android binaries into src/main/jniLibs/ and does not build anything.
# The binary resolution is handled by a Dart script that supports multiple methods:
# 1. Environment variable override (NOBODYWHO_FLUTTER_LIB_PATH)
# 2. Local cargo build detection
# 3. Cached download
# 4. Download from GitHub releases

# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

project(${PROJECT_NAME})

# NDK triple mapping (needed for libc++_shared.so path, which differs from Rust triples)
if(ANDROID_ABI STREQUAL "arm64-v8a")
  set(NDK_TRIPLE "aarch64-linux-android")
elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
  set(NDK_TRIPLE "arm-linux-androideabi")
elseif(ANDROID_ABI STREQUAL "x86")
  set(NDK_TRIPLE "i686-linux-android")
elseif(ANDROID_ABI STREQUAL "x86_64")
  set(NDK_TRIPLE "x86_64-linux-android")
else()
  message(FATAL_ERROR "Unsupported Android ABI: ${ANDROID_ABI}")
endif()

# Resolve binary using Dart script (per-ABI)
execute_process(
  COMMAND dart run "${CMAKE_CURRENT_SOURCE_DIR}/../tool/resolve_binary.dart"
          --platform=android
          --arch=${ANDROID_ABI}
          --build-type=release
          --cache-dir=${CMAKE_CURRENT_BINARY_DIR}
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/.."
  OUTPUT_VARIABLE NOBODYWHO_LIB_PATH
  OUTPUT_STRIP_TRAILING_WHITESPACE
  ERROR_VARIABLE resolve_error
  RESULT_VARIABLE resolve_result
)

if(NOT resolve_result EQUAL 0)
  message(FATAL_ERROR "Failed to resolve NobodyWho binary for ${ANDROID_ABI}:\n${resolve_error}")
endif()

message(STATUS "Using NobodyWho library for ${ANDROID_ABI}: ${NOBODYWHO_LIB_PATH}")

# Copy to jniLibs for Android packaging
set(LibRoot "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs")
file(REMOVE_RECURSE ${LibRoot}/${ANDROID_ABI})
file(MAKE_DIRECTORY ${LibRoot}/${ANDROID_ABI})
file(COPY ${NOBODYWHO_LIB_PATH} DESTINATION ${LibRoot}/${ANDROID_ABI})

# Copy libc++_shared.so from NDK
# CMAKE_ANDROID_NDK is automatically set by the Android Gradle Plugin
if(CMAKE_ANDROID_NDK)
  # Try modern NDK layout first
  set(CXX_SHARED_LIB "${CMAKE_ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/${NDK_TRIPLE}/libc++_shared.so")
  if(EXISTS ${CXX_SHARED_LIB})
    message(STATUS "Copying libc++_shared.so for ${ANDROID_ABI}")
    file(COPY ${CXX_SHARED_LIB} DESTINATION ${LibRoot}/${ANDROID_ABI})
  else()
    # Try alternative location for Windows/Mac or older NDK versions
    set(CXX_SHARED_LIB "${CMAKE_ANDROID_NDK}/sources/cxx-stl/llvm-libc++/libs/${ANDROID_ABI}/libc++_shared.so")
    if(EXISTS ${CXX_SHARED_LIB})
      message(STATUS "Copying libc++_shared.so for ${ANDROID_ABI} (alternative path)")
      file(COPY ${CXX_SHARED_LIB} DESTINATION ${LibRoot}/${ANDROID_ABI})
    else()
      message(WARNING "libc++_shared.so not found in NDK for ${ANDROID_ABI}")
    endif()
  endif()
else()
  message(WARNING "CMAKE_ANDROID_NDK not set, cannot copy libc++_shared.so")
endif()
