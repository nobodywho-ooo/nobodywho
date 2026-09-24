# llama-cpp-sys forwards CMAKE_PROJECT_INCLUDE. Scope this hook so it cannot
# affect the host shader generator or other dependency projects.
if(NOT ANDROID OR NOT PROJECT_NAME STREQUAL "llama.cpp")
    return()
endif()
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(GGML_BACKEND_DL OFF CACHE BOOL "" FORCE)
set(GGML_CPU_ALL_VARIANTS OFF CACHE BOOL "" FORCE)
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
set(GGML_OPENCL_USE_ADRENO_KERNELS OFF CACHE BOOL "" FORCE)
set(LLAMA_OPENSSL OFF CACHE BOOL "" FORCE)
if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(GGML_CPU_ARM_ARCH "armv8-a" CACHE STRING "" FORCE)
endif()

# Compile ggml-opencl against opencl-api.h, so its OpenCL calls go through
# NobodyWho's table (core/src/opencl.rs) instead of linking libOpenCL.so.
# Deferred to the end of this directory, once the backend target exists.
function(nobodywho_opencl_by_name)
    if(NOT GGML_OPENCL)
        return()
    endif()
    if(NOT TARGET ggml-opencl)
        message(FATAL_ERROR "GGML_OPENCL is on but the ggml-opencl target is missing; update ${CMAKE_CURRENT_FUNCTION_LIST_FILE}")
    endif()
    set(header "${CMAKE_CURRENT_FUNCTION_LIST_DIR}/opencl-api.h")
    target_compile_options(ggml-opencl PRIVATE "SHELL:-include \"${header}\"")
endfunction()
cmake_language(DEFER CALL nobodywho_opencl_by_name)
