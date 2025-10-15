# The Flutter tooling requires that developers have CMake 3.10 or later
# installed. You should not increase this version, as doing so will cause
# the plugin to fail to compile for some customers of the plugin.
cmake_minimum_required(VERSION 3.10)

# Project-level configuration.
set(PROJECT_NAME "nobodywho_flutter")
project(${PROJECT_NAME} LANGUAGES CXX)


# find library version by regex matching the pubspec file
file(READ "${CMAKE_CURRENT_SOURCE_DIR}/../pubspec.yaml" pubspec_content)
if(pubspec_content MATCHES "version: ([0-9]+\\.[0-9]+\\.[0-9]+)")
  set(LibraryVersion ${CMAKE_MATCH_1})
else()
  message(FATAL_ERROR "Could not parse version from pubspec.yaml. Please ensure the file exists and contains a valid version field.")
endif()
message(STATUS "Using LibraryVersion: ${LibraryVersion}")


# try to find binary shared lib in three different ways
set(NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/release/libnobodywho_flutter.so")

# 1. check env var
if(DEFINED ENV{NOBODYWHO_FLUTTER_LIB_PATH})
  set(NOBODYWHO_FLUTTER_LIB_PATH $ENV{NOBODYWHO_FLUTTER_LIB_PATH})

# 2. check local build
elseif(EXISTS ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})
  message("Using locally built NobodyWho artifact")
  set(NOBODYWHO_FLUTTER_LIB_PATH ${NOBODYWHO_FLUTTER_LOCAL_BUILD_LIB_PATH})

# 3. download from github assets
else()
  # Create a directory for downloaded artifacts
  set(DOWNLOAD_DIR "${CMAKE_CURRENT_BINARY_DIR}/downloaded")
  file(MAKE_DIRECTORY ${DOWNLOAD_DIR})

  set(DOWNLOAD_FILE "${DOWNLOAD_DIR}/nobodywho_flutter_${LibraryVersion}.tar.gz")
  set(EXTRACTED_LIB "${DOWNLOAD_DIR}/libnobodywho_flutter.so")

  # Download if not already present
  if(NOT EXISTS ${EXTRACTED_LIB})
    message(STATUS "Downloading NobodyWho ${LibraryVersion} from github...")
    file(DOWNLOAD
      "https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-${LibraryVersion}/nobodywho_flutter_bundled_libs.tar.gz"
      ${DOWNLOAD_FILE}
      TLS_VERIFY ON
      STATUS download_status
      SHOW_PROGRESS
    )

    list(GET download_status 0 status_code)
    list(GET download_status 1 status_string)

    if(NOT status_code EQUAL 0)
      message(FATAL_ERROR "Failed to download NobodyWho library: ${status_string}")
    endif()

    message(STATUS "Extracting downloaded library...")
    # Extract the tar.gz file
    execute_process(
      COMMAND ${CMAKE_COMMAND} -E tar xzf ${DOWNLOAD_FILE}
      WORKING_DIRECTORY ${DOWNLOAD_DIR}
      RESULT_VARIABLE extract_result
    )

    if(NOT extract_result EQUAL 0)
      message(FATAL_ERROR "Failed to extract NobodyWho library")
    endif()

    if(NOT EXISTS ${EXTRACTED_LIB})
      message(FATAL_ERROR "Expected library file not found after extraction: ${EXTRACTED_LIB}")
    endif()
  else()
    message(STATUS "Using cached NobodyWho library from: ${EXTRACTED_LIB}")
  endif()

  set(NOBODYWHO_FLUTTER_LIB_PATH ${EXTRACTED_LIB})

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
