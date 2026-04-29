cmake_minimum_required(VERSION 3.13)

project(emnapi LANGUAGES C ASM)

option(EMNAPI_INSTALL_SRC "EMNAPI_INSTALL_SRC" OFF)
option(EMNAPI_FIND_NODE_ADDON_API "EMNAPI_FIND_NODE_ADDON_API" OFF)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  set(IS_EMSCRIPTEN ON)
else()
  set(IS_EMSCRIPTEN OFF)
endif()

if((CMAKE_C_COMPILER_TARGET STREQUAL "wasm32") OR (CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-unknown-unknown"))
  set(IS_WASM32 ON)
else()
  set(IS_WASM32 OFF)
endif()

if((CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-wasi") OR (CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-wasip1"))
  set(IS_WASM32_WASIP1 ON)
else()
  set(IS_WASM32_WASIP1 OFF)
endif()

if((CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-wasi-threads") OR (CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-wasip1-threads"))
  set(IS_WASM32_WASIP1_THREADS ON)
else()
  set(IS_WASM32_WASIP1_THREADS OFF)
endif()

set(UV_SRC
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/uv-common.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/threadpool.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/unix/loop.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/unix/posix-hrtime.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/unix/thread.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/unix/async.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/uv/unix/core.c"
)

set(ENAPI_BASIC_SRC
  "${CMAKE_CURRENT_SOURCE_DIR}/src/js_native_api.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/node_api.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/async_cleanup_hook.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/async_context.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/wasi_wait.c"
)
set(EMNAPI_THREADS_SRC
  "${CMAKE_CURRENT_SOURCE_DIR}/src/async_work.c"
  "${CMAKE_CURRENT_SOURCE_DIR}/src/threadsafe_function.c"
)
set(EMNAPI_SRC ${ENAPI_BASIC_SRC} ${EMNAPI_THREADS_SRC})

set(EMNAPI_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/include/node")
set(EMNAPI_DEFINES "BUILDING_NODE_EXTENSION")

set(EMNAPI_JS_LIB "${CMAKE_CURRENT_SOURCE_DIR}/dist/library_napi.js")

if(IS_EMSCRIPTEN)
  set(EMNAPI_MT_CFLAGS "-pthread" "-sWASM_WORKERS=1")

  # https://github.com/emscripten-core/emscripten/issues/20035
  # https://github.com/emscripten-core/emscripten/pull/20130
  list(APPEND EMNAPI_DEFINES "NAPI_EXTERN=__attribute__((__import_module__(\"env\")))")
else()
  set(EMNAPI_MT_CFLAGS "-pthread")
endif()

set(EMNAPI_BASIC_TARGET_NAME "emnapi-basic")
set(EMNAPI_BASIC_MT_TARGET_NAME "emnapi-basic-mt")
set(EMNAPI_TARGET_NAME "emnapi")
set(EMNAPI_MT_TARGET_NAME "emnapi-mt")

set(EMNAPI_BASIC_NAPI_RS_MT_TARGET_NAME "emnapi-basic-napi-rs-mt")
set(EMNAPI_NAPI_RS_MT_TARGET_NAME "emnapi-napi-rs-mt")

set(DLMALLOC_TARGET_NAME "dlmalloc")
set(DLMALLOC_MT_TARGET_NAME "dlmalloc-mt")
set(EMMALLOC_TARGET_NAME "emmalloc")
set(EMMALLOC_MT_TARGET_NAME "emmalloc-mt")

if(EMNAPI_FIND_NODE_ADDON_API)
  execute_process(
    COMMAND "node" "-p" "require('node-addon-api').include"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    OUTPUT_VARIABLE OUTPUT_NAA_INCLUDE_DIR
    ERROR_VARIABLE ERROR_NAA_INCLUDE_DIR
  )
  if(NOT (ERROR_NAA_INCLUDE_DIR STREQUAL ""))
    message(WARNING "Cannot find module 'node-addon-api'")
  else()
    string(REGEX REPLACE "(\r?\n)|\"" "" OUTPUT_NAA_INCLUDE_DIR "${OUTPUT_NAA_INCLUDE_DIR}")
    string(REPLACE "\\" "/" OUTPUT_NAA_INCLUDE_DIR "${OUTPUT_NAA_INCLUDE_DIR}")
    list(APPEND EMNAPI_INCLUDE "${OUTPUT_NAA_INCLUDE_DIR}")
  endif()
endif()

if(IS_WASM32)
  set(MALLOC_PUBLIC_SOURCES
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/sbrk.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/memcpy.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/memset.c"
  )
  add_library(${DLMALLOC_TARGET_NAME} STATIC
    ${MALLOC_PUBLIC_SOURCES}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/dlmalloc/dlmalloc.c"
  )
  target_compile_definitions(${DLMALLOC_TARGET_NAME} PRIVATE "PAGESIZE=65536")

  add_library(${DLMALLOC_MT_TARGET_NAME} STATIC
    ${MALLOC_PUBLIC_SOURCES}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/dlmalloc/dlmalloc.c"
  )
  target_compile_options(${DLMALLOC_MT_TARGET_NAME} PUBLIC "-matomics" "-mbulk-memory")
  target_compile_definitions(${DLMALLOC_MT_TARGET_NAME} PRIVATE "PAGESIZE=65536" "USE_LOCKS=1")

  add_library(${EMMALLOC_TARGET_NAME} STATIC
    ${MALLOC_PUBLIC_SOURCES}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/emmalloc/emmalloc.c"
  )
  target_compile_options(${EMMALLOC_TARGET_NAME} PRIVATE "-fno-strict-aliasing")
  target_compile_definitions(${EMMALLOC_TARGET_NAME} PRIVATE "PAGESIZE=65536")

  add_library(${EMMALLOC_MT_TARGET_NAME} STATIC
    ${MALLOC_PUBLIC_SOURCES}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/malloc/emmalloc/emmalloc.c"
  )
  target_compile_options(${EMMALLOC_MT_TARGET_NAME} PRIVATE "-fno-strict-aliasing")
  target_compile_options(${EMMALLOC_MT_TARGET_NAME} PUBLIC "-matomics" "-mbulk-memory")
  target_compile_definitions(${EMMALLOC_MT_TARGET_NAME} PRIVATE "PAGESIZE=65536" "__EMSCRIPTEN_SHARED_MEMORY__=1")
endif()

if(NAPI_VERSION)
  add_compile_definitions("NAPI_VERSION=${NAPI_VERSION}")
endif()
if(NAPI_EXPERIMENTAL)
  add_compile_definitions("NAPI_EXPERIMENTAL=${NAPI_EXPERIMENTAL}")
endif()
if(NODE_API_EXPERIMENTAL_NO_WARNING)
  add_compile_definitions("NODE_API_EXPERIMENTAL_NO_WARNING=${NODE_API_EXPERIMENTAL_NO_WARNING}")
endif()
if(NODE_WANT_INTERNALS)
  add_compile_definitions("NODE_WANT_INTERNALS=${NODE_WANT_INTERNALS}")
endif()
if(EMNAPI_WORKER_POOL_SIZE)
  add_compile_definitions("EMNAPI_WORKER_POOL_SIZE=${EMNAPI_WORKER_POOL_SIZE}")
endif()
if(EMNAPI_NEXTTICK_TYPE)
  add_compile_definitions("EMNAPI_NEXTTICK_TYPE=${EMNAPI_NEXTTICK_TYPE}")
endif()
if(EMNAPI_USE_PROXYING)
  add_compile_definitions("EMNAPI_USE_PROXYING=${EMNAPI_USE_PROXYING}")
endif()

add_library(${EMNAPI_TARGET_NAME} STATIC ${EMNAPI_SRC} ${UV_SRC})
target_include_directories(${EMNAPI_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
target_compile_definitions(${EMNAPI_TARGET_NAME} PUBLIC ${EMNAPI_DEFINES})
if(IS_EMSCRIPTEN)
  set_target_properties(${EMNAPI_TARGET_NAME} PROPERTIES INTERFACE_LINK_DEPENDS ${EMNAPI_JS_LIB})
  target_link_options(${EMNAPI_TARGET_NAME} INTERFACE "--js-library=${EMNAPI_JS_LIB}")
endif()

add_library(${EMNAPI_BASIC_TARGET_NAME} STATIC ${ENAPI_BASIC_SRC} ${UV_SRC})
target_include_directories(${EMNAPI_BASIC_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
target_compile_definitions(${EMNAPI_BASIC_TARGET_NAME}
  PUBLIC ${EMNAPI_DEFINES}
)
if(IS_EMSCRIPTEN)
  set_target_properties(${EMNAPI_BASIC_TARGET_NAME} PROPERTIES INTERFACE_LINK_DEPENDS ${EMNAPI_JS_LIB})
  target_link_options(${EMNAPI_BASIC_TARGET_NAME} INTERFACE "--js-library=${EMNAPI_JS_LIB}")
endif()

if(IS_WASM32 OR IS_WASM32_WASIP1 OR IS_WASM32_WASIP1_THREADS)
  set(EMNAPI_BUILD_BASIC_MT ON)
else()
  set(EMNAPI_BUILD_BASIC_MT OFF)
endif()

if(IS_WASM32_WASIP1_THREADS)
  set(EMNAPI_BUILD_FOR_NAPI_RS ON)
else()
  set(EMNAPI_BUILD_FOR_NAPI_RS OFF)
endif()

if(EMNAPI_BUILD_FOR_NAPI_RS)
  add_library(${EMNAPI_NAPI_RS_MT_TARGET_NAME} STATIC ${EMNAPI_SRC} ${UV_SRC})
  target_include_directories(${EMNAPI_NAPI_RS_MT_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
  target_compile_definitions(${EMNAPI_NAPI_RS_MT_TARGET_NAME}
    PUBLIC ${EMNAPI_DEFINES} "NAPI_EXTERN=__attribute__((__import_module__(\"env\")))"
  )

  add_library(${EMNAPI_BASIC_NAPI_RS_MT_TARGET_NAME} STATIC
    ${ENAPI_BASIC_SRC}
    ${UV_SRC}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/thread/async_worker_create.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/thread/async_worker_init.S"
  )
  target_include_directories(${EMNAPI_BASIC_NAPI_RS_MT_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
  target_compile_definitions(${EMNAPI_BASIC_NAPI_RS_MT_TARGET_NAME}
    PUBLIC ${EMNAPI_DEFINES} "NAPI_EXTERN=__attribute__((__import_module__(\"env\")))"
  )
endif()

if(EMNAPI_BUILD_BASIC_MT)
  add_library(${EMNAPI_BASIC_MT_TARGET_NAME} STATIC
    ${ENAPI_BASIC_SRC}
    ${UV_SRC}
    "${CMAKE_CURRENT_SOURCE_DIR}/src/thread/async_worker_create.c"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/thread/async_worker_init.S"
  )
  target_compile_options(${EMNAPI_BASIC_MT_TARGET_NAME} PUBLIC "-matomics" "-mbulk-memory")
  target_include_directories(${EMNAPI_BASIC_MT_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
  target_compile_definitions(${EMNAPI_BASIC_MT_TARGET_NAME}
    PUBLIC ${EMNAPI_DEFINES}
  )

  if(IS_EMSCRIPTEN)
    set_target_properties(${EMNAPI_BASIC_MT_TARGET_NAME} PROPERTIES INTERFACE_LINK_DEPENDS ${EMNAPI_JS_LIB})
    target_link_options(${EMNAPI_BASIC_MT_TARGET_NAME} INTERFACE "--js-library=${EMNAPI_JS_LIB}")
  endif()
endif()

if(IS_EMSCRIPTEN OR IS_WASM32_WASIP1_THREADS)
  set(EMNAPI_BUILD_MT ON)
else()
  set(EMNAPI_BUILD_MT OFF)
endif()

if(EMNAPI_BUILD_MT)
  add_library(${EMNAPI_MT_TARGET_NAME} STATIC ${EMNAPI_SRC} ${UV_SRC})
  target_compile_options(${EMNAPI_MT_TARGET_NAME}
    PUBLIC "-matomics" "-mbulk-memory"
    PRIVATE ${EMNAPI_MT_CFLAGS}
  )
  target_include_directories(${EMNAPI_MT_TARGET_NAME} PUBLIC ${EMNAPI_INCLUDE})
  target_compile_definitions(${EMNAPI_MT_TARGET_NAME} PUBLIC ${EMNAPI_DEFINES})
  if(IS_EMSCRIPTEN)
    set_target_properties(${EMNAPI_MT_TARGET_NAME} PROPERTIES INTERFACE_LINK_DEPENDS ${EMNAPI_JS_LIB})
    target_link_options(${EMNAPI_MT_TARGET_NAME} INTERFACE "--js-library=${EMNAPI_JS_LIB}")
  endif()
endif()

if(IS_EMSCRIPTEN)
  set(LIB_ARCH "${CMAKE_LIBRARY_ARCHITECTURE}")
else()
  set(LIB_ARCH "${CMAKE_C_COMPILER_TARGET}")
endif()

# install(TARGETS ${EMNAPI_TARGET_NAME} DESTINATION "lib/${PROJECT_NAME}")
# if(EMNAPI_BUILD_MT)
#   install(TARGETS ${EMNAPI_MT_TARGET_NAME} DESTINATION "lib/${PROJECT_NAME}")
# endif()
if(LIB_ARCH)
  install(TARGETS ${EMNAPI_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  install(TARGETS ${EMNAPI_BASIC_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  if(EMNAPI_BUILD_MT)
    install(TARGETS ${EMNAPI_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  endif()
  if(EMNAPI_BUILD_BASIC_MT)
    install(TARGETS ${EMNAPI_BASIC_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  endif()
  if(EMNAPI_BUILD_FOR_NAPI_RS)
    install(TARGETS ${EMNAPI_NAPI_RS_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
    install(TARGETS ${EMNAPI_BASIC_NAPI_RS_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  endif()
  if(IS_WASM32)
    install(TARGETS ${DLMALLOC_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
    install(TARGETS ${DLMALLOC_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
    install(TARGETS ${EMMALLOC_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
    install(TARGETS ${EMMALLOC_MT_TARGET_NAME} DESTINATION "lib/${LIB_ARCH}")
  endif()
endif()

install(FILES
    ${CMAKE_CURRENT_SOURCE_DIR}/common.gypi
    ${CMAKE_CURRENT_SOURCE_DIR}/emnapi.gyp
  DESTINATION ".")

install(FILES
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/emnapi_common.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/emnapi.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/js_native_api_types.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/js_native_api.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/node_api_types.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/node_api.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/uv.h
    ${CMAKE_CURRENT_SOURCE_DIR}/include/node/config.gypi
  DESTINATION "include/node")

install(DIRECTORY
  ${CMAKE_CURRENT_SOURCE_DIR}/include/node/uv
  DESTINATION "include/node")

install(FILES
    ${CMAKE_CURRENT_SOURCE_DIR}/dist/library_napi.js
  DESTINATION "dist")

if(EMNAPI_INSTALL_SRC)
  install(FILES
      ${EMNAPI_SRC}
      "${CMAKE_CURRENT_SOURCE_DIR}/src/emnapi_internal.h"
    DESTINATION "src")
  install(DIRECTORY
    ${CMAKE_CURRENT_SOURCE_DIR}/src/uv
    DESTINATION "src")
  install(DIRECTORY
    ${CMAKE_CURRENT_SOURCE_DIR}/src/malloc
    DESTINATION "src")
  install(DIRECTORY
    ${CMAKE_CURRENT_SOURCE_DIR}/src/thread
    DESTINATION "src")
endif()
