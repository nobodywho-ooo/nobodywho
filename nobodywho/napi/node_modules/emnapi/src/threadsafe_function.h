#ifndef SRC_THREADSAFE_FUNCTION_H_
#define SRC_THREADSAFE_FUNCTION_H_

#include "node_api.h"
#include "emnapi_internal.h"

#include <stdatomic.h>
#include <pthread.h>
#include <errno.h>
#include "uv.h"
#include "uv/queue.h"

typedef struct optional_async_resource {
  ASYNC_RESOURCE_FIELD
  bool is_some;
} optional_async_resource;

typedef enum napi_threadsafe_function_state {
  napi_tsfn_state_open,
  napi_tsfn_state_closing,
  napi_tsfn_state_closed
} napi_threadsafe_function_state;

struct napi_threadsafe_function__ {
  optional_async_resource async_resource;
  // These are variables protected by the mutex.
  pthread_mutex_t mutex;
  pthread_cond_t* cond;
  size_t queue_size;
  struct uv__queue queue;
  uv_async_t async;
  size_t thread_count;
  napi_threadsafe_function_state state;
  atomic_uchar dispatch_state;

  // These are variables set once, upon creation, and then never again, which
  // means we don't need the mutex to read them.
  void* context;
  size_t max_queue_size;

  // These are variables accessed only from the loop thread.
  napi_ref ref;
  napi_env env;
  void* finalize_data;
  napi_finalize finalize_cb;
  napi_threadsafe_function_call_js call_js_cb;
  bool handles_closing;
  unsigned int async_ref;
};

#endif
