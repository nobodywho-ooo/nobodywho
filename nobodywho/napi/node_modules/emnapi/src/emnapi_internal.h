#ifndef EMNAPI_INTERNAL_H
#define EMNAPI_INTERNAL_H

#include "emnapi.h"

#if defined(__EMSCRIPTEN__) || defined(__wasi__)
#include <assert.h>
#include <stdlib.h>
#else
#if __STDC_VERSION__ >= 201112L && !defined(__cplusplus)
#define static_assert _Static_assert
#endif

#include <stddef.h>

EXTERN_C_START
void* malloc(size_t size);
void* calloc(size_t count, size_t size);
void free(void* p);
EXTERN_C_END
#endif

#ifdef __EMSCRIPTEN__
#include <emscripten.h>

#if __EMSCRIPTEN_major__ * 10000 + __EMSCRIPTEN_minor__ * 100 + __EMSCRIPTEN_tiny__ >= 30114  // NOLINT
#include <emscripten/eventloop.h>
#endif
#endif

#ifdef NDEBUG
  #define	EMNAPI_ASSERT_CALL(the_call) (the_call)
#else
  #if defined(__EMSCRIPTEN__) || defined(__wasi__)
  #define EMNAPI_ASSERT_CALL(the_call) (assert(napi_ok == (the_call)))
  #else
  #define	EMNAPI_ASSERT_CALL(the_call) (the_call)
  #endif
#endif

#define CHECK_ENV(env)          \
  do {                          \
    if ((env) == NULL) {        \
      return napi_invalid_arg;  \
    }                           \
  } while (0)

#define RETURN_STATUS_IF_FALSE(env, condition, status)                  \
  do {                                                                  \
    if (!(condition)) {                                                 \
      return napi_set_last_error((env), (status), 0, NULL);             \
    }                                                                   \
  } while (0)

#define CHECK_ARG(env, arg) \
  RETURN_STATUS_IF_FALSE((env), ((arg) != NULL), napi_invalid_arg)

#define CHECK_ENV_NOT_IN_GC(env)                                               \
  do {                                                                         \
    CHECK_ENV((env));                                                          \
    _emnapi_env_check_gc_access((env));                                          \
  } while (0)

#define CHECK(expr)                                                           \
  do {                                                                        \
    if (!(expr)) {                                                            \
      __builtin_trap();                                                       \
    }                                                                         \
  } while (0)

#define CHECK_NOT_NULL(val) CHECK((val) != NULL)

#define CHECK_EQ(a, b) CHECK((a) == (b))
#define CHECK_LE(a, b) CHECK((a) <= (b))

EXTERN_C_START

EMNAPI_INTERNAL_EXTERN napi_status napi_set_last_error(node_api_basic_env env,
                                       napi_status error_code,
                                       uint32_t engine_error_code,
                                       void* engine_reserved);
EMNAPI_INTERNAL_EXTERN napi_status napi_clear_last_error(node_api_basic_env env);

#ifdef __EMSCRIPTEN__
#if __EMSCRIPTEN_major__ * 10000 + __EMSCRIPTEN_minor__ * 100 + __EMSCRIPTEN_tiny__ >= 30114  // NOLINT
#define EMNAPI_KEEPALIVE_PUSH emscripten_runtime_keepalive_push
#define EMNAPI_KEEPALIVE_POP emscripten_runtime_keepalive_pop
#else

EMNAPI_INTERNAL_EXTERN void _emnapi_runtime_keepalive_push();
EMNAPI_INTERNAL_EXTERN void _emnapi_runtime_keepalive_pop();

#define EMNAPI_KEEPALIVE_PUSH _emnapi_runtime_keepalive_push
#define EMNAPI_KEEPALIVE_POP _emnapi_runtime_keepalive_pop
#endif
#else
EMNAPI_INTERNAL_EXTERN void _emnapi_runtime_keepalive_push();
EMNAPI_INTERNAL_EXTERN void _emnapi_runtime_keepalive_pop();

#define EMNAPI_KEEPALIVE_PUSH _emnapi_runtime_keepalive_push
#define EMNAPI_KEEPALIVE_POP _emnapi_runtime_keepalive_pop
#endif

EMNAPI_INTERNAL_EXTERN napi_handle_scope _emnapi_open_handle_scope();
EMNAPI_INTERNAL_EXTERN void _emnapi_close_handle_scope(napi_handle_scope scope);
EMNAPI_INTERNAL_EXTERN void _emnapi_env_ref(napi_env env);
EMNAPI_INTERNAL_EXTERN void _emnapi_env_unref(napi_env env);
EMNAPI_INTERNAL_EXTERN void _emnapi_ctx_increase_waiting_request_counter();
EMNAPI_INTERNAL_EXTERN void _emnapi_ctx_decrease_waiting_request_counter();

EMNAPI_INTERNAL_EXTERN int _emnapi_is_main_browser_thread();
EMNAPI_INTERNAL_EXTERN int _emnapi_is_main_runtime_thread();
EMNAPI_INTERNAL_EXTERN double _emnapi_get_now();
EMNAPI_INTERNAL_EXTERN void _emnapi_unwind();
EMNAPI_INTERNAL_EXTERN void _emnapi_set_immediate(void (*callback)(void*), void* data);
EMNAPI_INTERNAL_EXTERN napi_status _emnapi_create_function(napi_env env,
                                                           const char* utf8name,
                                                           size_t length,
                                                           napi_callback cb,
                                                           void* data,
                                                           napi_value* result);

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)
#define EMNAPI_HAVE_THREADS 1
#else
#define EMNAPI_HAVE_THREADS 0
#endif

#if EMNAPI_HAVE_THREADS

#define container_of(ptr, type, member) \
  ((type *) ((char *) (ptr) - offsetof(type, member)))

typedef void (*_emnapi_call_into_module_callback)(napi_env env, void* args);
EMNAPI_INTERNAL_EXTERN
void _emnapi_callback_into_module(int force_uncaught,
                                  napi_env env,
                                  _emnapi_call_into_module_callback callback,
                                  void* args,
                                  int close_scope_if_throw);

#endif

typedef double async_id_t;
typedef struct async_context {
  async_id_t async_id;
  async_id_t trigger_async_id;
} async_context;

#define ASYNC_RESOURCE_FIELD             \
  napi_ref resource_;                    \
  async_context async_context_;

typedef struct emnapi_async_resource {
  ASYNC_RESOURCE_FIELD
} emnapi_async_resource;

// call node::EmitAsyncInit
EMNAPI_INTERNAL_EXTERN
void _emnapi_node_emit_async_init(napi_value async_resource,
                                  napi_value async_resource_name,
                                  async_id_t trigger_async_id,
                                  async_context* result);
// call node::EmitAsyncDestroy
EMNAPI_INTERNAL_EXTERN
void _emnapi_node_emit_async_destroy(async_id_t id, async_id_t trigger_async_id);

// EMNAPI_INTERNAL_EXTERN void _emnapi_node_open_callback_scope(napi_value async_resource, async_id_t id, async_id_t trigger_async_id, int64_t* result);
// EMNAPI_INTERNAL_EXTERN void _emnapi_node_close_callback_scope(int64_t* scope);

// call node:MakeCallback
EMNAPI_INTERNAL_EXTERN
napi_status _emnapi_node_make_callback(napi_env env,
                                       napi_value async_resource,
                                       napi_value cb,
                                       napi_value* argv,
                                       size_t size,
                                       double async_id,
                                       double trigger_async_id,
                                       napi_value* result);

EMNAPI_INTERNAL_EXTERN
void _emnapi_env_check_gc_access(napi_env env);

#define EMNAPI_ASYNC_RESOURCE_CTOR(env, res, name, ar) \
  do { \
    EMNAPI_ASSERT_CALL(napi_create_reference((env), (res), 1, &(ar)->resource_)); \
    _emnapi_node_emit_async_init((res), (name), -1.0, &(ar)->async_context_); \
  } while (0)

#define EMNAPI_ASYNC_RESOURCE_DTOR(env, ar) \
  do { \
    EMNAPI_ASSERT_CALL(napi_delete_reference((env), (ar)->resource_)); \
    _emnapi_node_emit_async_destroy((ar)->async_context_.async_id, \
                                    (ar)->async_context_.trigger_async_id); \
  } while (0)

EXTERN_C_END

#endif
