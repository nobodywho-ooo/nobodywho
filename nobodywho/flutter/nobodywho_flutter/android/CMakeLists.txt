set(PROJECT_NAME "nobodywho_flutter")

# Unlike the Windows & Linux CMakeLists.txt, this Android equivalent is just here
# to download the Android binaries into src/main/jniLibs/ and does not build anything.
# The binary download/extraction is difficult to do concisely in Groovy/Gradle,
# at least across host platforms, so we are just reusing our Linux/Windows logic.

# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

project(PROJECT_NAME)

if(ANDROID_ABI STREQUAL "arm64-v8a")
  set(RUST_TARGET_TRIPLE "aarch64-linux-android")
  set(NDK_TRIPLE "aarch64-linux-android")
elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
  set(RUST_TARGET_TRIPLE "armv7-linux-androideabi")
  set(NDK_TRIPLE "arm-linux-androideabi")  # Different from Rust!
elseif(ANDROID_ABI STREQUAL "x86")
  set(RUST_TARGET_TRIPLE "i686-linux-android")
  set(NDK_TRIPLE "i686-linux-android")
elseif(ANDROID_ABI STREQUAL "x86_64")
  set(RUST_TARGET_TRIPLE "x86_64-linux-android")
  set(NDK_TRIPLE "x86_64-linux-android")
endif()

# Read version from pubspec.yaml (single source of truth)
set(PUBSPEC_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../pubspec.yaml")
file(READ ${PUBSPEC_FILE} PUBSPEC_CONTENT)
# Match: MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]
# Examples: 0.2.43, 0.0.0-rc1, 0.2.43-dev.4, 0.2.43+1, 0.2.43-alpha.12+hotfix.oopsie
string(REGEX MATCH "version: *([0-9]+\\.[0-9]+\\.[0-9]+(-[a-zA-Z0-9._-]+)?(\\+[a-zA-Z0-9._-]+)?)" _ ${PUBSPEC_CONTENT})
set(LIBRARY_VERSION ${CMAKE_MATCH_1})

if(NOT LIBRARY_VERSION)
    message(FATAL_ERROR "Could not parse version from ${PUBSPEC_FILE}")
endif()

message("NobodyWho Flutter Plugin version: ${LIBRARY_VERSION}")

set(NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/${RUST_TARGET_TRIPLE}/release/libnobodywho_flutter.so")
set(LibRoot "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs")

# Try to find the library using multiple methods
set(NOBODYWHO_FLUTTER_LIB_PATH "")

# Method 1: check environment variable
if(DEFINED ENV{NOBODYWHO_FLUTTER_LIB_PATH})
  if(EXISTS "$ENV{NOBODYWHO_FLUTTER_LIB_PATH}")
    message("Using NobodyWho artifact from environment variable: $ENV{NOBODYWHO_FLUTTER_LIB_PATH}")
    set(NOBODYWHO_FLUTTER_LIB_PATH "$ENV{NOBODYWHO_FLUTTER_LIB_PATH}")
  else()
    message(WARNING "NOBODYWHO_FLUTTER_LIB_PATH is set but file does not exist: $ENV{NOBODYWHO_FLUTTER_LIB_PATH}")
  endif()
endif()

# Method 2: check local build
if(NOT NOBODYWHO_FLUTTER_LIB_PATH AND EXISTS ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})
  message("Using locally built NobodyWho artifact: ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH}")
  set(NOBODYWHO_FLUTTER_LIB_PATH ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})
endif()

# Method 3: try to download
if(NOT NOBODYWHO_FLUTTER_LIB_PATH)
  # TODO: this won't actually work until we can make real releases
  set(DOWNLOAD_PATH "${CMAKE_CURRENT_BINARY_DIR}/libnobodywho_flutter.so")
  set(DOWNLOAD_URL "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-v${LIBRARY_VERSION}/libnobodywho-flutter-${RUST_TARGET_TRIPLE}-release.so")
  message("Attempting to download NobodyWho from github url: ${DOWNLOAD_URL}")

  file(DOWNLOAD
    "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-v${LIBRARY_VERSION}/libnobodywho-flutter-aarch64-linux-android-release.so"
    ${DOWNLOAD_PATH}
    TLS_VERIFY ON
    STATUS DOWNLOAD_STATUS
  )
  list(GET DOWNLOAD_STATUS 0 DOWNLOAD_STATUS_CODE)
  if(DOWNLOAD_STATUS_CODE EQUAL 0 AND EXISTS ${DOWNLOAD_PATH})
    message("Successfully downloaded NobodyWho artifact")
    set(NOBODYWHO_FLUTTER_LIB_PATH ${DOWNLOAD_PATH})
  else()
    message(WARNING "Download failed: ${DOWNLOAD_STATUS}")
  endif()
endif()

# If all methods failed, error out
if(NOT NOBODYWHO_FLUTTER_LIB_PATH)
  message(FATAL_ERROR "libnobodywho_flutter.so not found!\n"
    "Tried:\n"
    "  1. Environment variable NOBODYWHO_FLUTTER_LIB_PATH\n"
    "  2. Local build at: ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH}\n"
    "  3. Download from GitHub releases\n"
    "\n"
    "Please build the Rust library first with:\n"
    "  cargo build --release --target aarch64-linux-android\n"
    "Or set NOBODYWHO_FLUTTER_LIB_PATH environment variable.")
endif()

# copy in the binary, overriding any already present.
file(REMOVE_RECURSE ${LibRoot})
file(MAKE_DIRECTORY ${LibRoot}/arm64-v8a)
file(COPY ${NOBODYWHO_FLUTTER_LIB_PATH} DESTINATION ${LibRoot}/arm64-v8a)

# Copy libc++_shared.so from NDK
# CMAKE_ANDROID_NDK is automatically set by the Android Gradle Plugin
set(CXX_SHARED_LIB "${CMAKE_ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so")
if(EXISTS ${CXX_SHARED_LIB})
  message("Copying libc++_shared.so to jniLibs")
  file(COPY ${CXX_SHARED_LIB} DESTINATION ${LibRoot}/arm64-v8a)
else()
  # Try alternative location for Windows/Mac
  set(CXX_SHARED_LIB "${CMAKE_ANDROID_NDK}/sources/cxx-stl/llvm-libc++/libs/arm64-v8a/libc++_shared.so")
  if(EXISTS ${CXX_SHARED_LIB})
    message("Copying libc++_shared.so to jniLibs (alternative path)")
    file(COPY ${CXX_SHARED_LIB} DESTINATION ${LibRoot}/arm64-v8a)
  else()
    message(WARNING "libc++_shared.so not found in NDK at ${CMAKE_ANDROID_NDK}")
  endif()
endif()
