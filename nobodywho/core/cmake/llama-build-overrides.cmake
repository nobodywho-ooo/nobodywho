# CMAKE_PROJECT_INCLUDE is process-wide, so scope the overrides to the exact
# llama.cpp Android build that needs loadable CPU backends.
if(NOT ANDROID OR NOT GGML_BACKEND_DL OR NOT PROJECT_NAME STREQUAL "llama.cpp")
  return()
endif()

set(LLAMA_OPENSSL OFF CACHE BOOL "" FORCE)
set(GGML_CPU_ARM_ARCH "" CACHE STRING "" FORCE)
set(CMAKE_PLATFORM_NO_VERSIONED_SONAME ON)
