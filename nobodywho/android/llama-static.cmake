# llama-cpp-sys forwards CMAKE_PROJECT_INCLUDE. Scope this hook so it cannot
# affect the host shader generator or other dependency projects.
if(NOT ANDROID OR NOT PROJECT_NAME STREQUAL "llama.cpp")
    return()
endif()
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(GGML_BACKEND_DL OFF CACHE BOOL "" FORCE)
set(GGML_CPU_ALL_VARIANTS OFF CACHE BOOL "" FORCE)
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(LLAMA_OPENSSL OFF CACHE BOOL "" FORCE)
if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(GGML_CPU_ARM_ARCH "armv8-a" CACHE STRING "" FORCE)
endif()
