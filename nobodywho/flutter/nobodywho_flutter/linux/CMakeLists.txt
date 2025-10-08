# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

# Project-level configuration.
set(PROJECT_NAME "nobodywho_flutter")
project(${PROJECT_NAME} LANGUAGES CXX)


set(NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/release/libnobodywho_flutter.so")

# check env var
if(DEFINED ENV{NOBODYWHO_FLUTTER_LIB_PATH})
  set(NOBODYWHO_FLUTTER_LIB_PATH $ENV{NOBODYWHO_FLUTTER_LIB_PATH})

# check local build
elseif(EXISTS ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})
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

# List of absolute paths to libraries that should be bundled with the plugin.
# This list could contain prebuilt libraries, or libraries created by an
# external build triggered from this build file.
set(nobodywho_flutter_bundled_libraries
  # Defined in ../src/CMakeLists.txt.
  # This can be changed to accommodate different builds.
  # $<TARGET_FILE:nobodywho_flutter>
  # "../../target/release/libnobodywho_flutter.so "
  ${NOBODYWHO_FLUTTER_LIB_PATH}
  PARENT_SCOPE
)
