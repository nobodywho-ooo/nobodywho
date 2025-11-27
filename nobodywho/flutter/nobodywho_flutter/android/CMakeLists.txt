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

set(NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/aarch64-linux-android/release/libnobodywho_flutter.so")
set(LibRoot "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs")

# check local build
if(EXISTS ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})
  message("Using locally built NobodyWho artifact")
  set(NOBODYWHO_FLUTTER_LIB_PATH ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})

else()
  # TODO: download
  # this won't actually work until we can make real releases
  # w/e
  message("Downloading NobodyWho from github...")
  set(NOBODYWHO_FLUTTER_LIB_PATH "./libnobodywho_flutter.so")
  file(DOWNLOAD
    "https://github.com/nobodywho-ooo/nobodywho/releases/download/${LibraryVersion}/nobodywho_flutter_bundled_libs.tar.gz"
    ${NOBODYWHO_FLUTTER_LIB_PATH}
    TLS_VERIFY ON
  )
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
