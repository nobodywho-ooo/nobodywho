#include "node_api.h"
#include "emnapi_internal.h"

#if EMNAPI_HAVE_THREADS

#include <pthread.h>
#include <errno.h>
#include "uv.h"

EXTERN_C_START

struct napi_async_work__ {
  ASYNC_RESOURCE_FIELD
  uv_work_t work_req_;
  napi_env env;
  void* data;
  napi_async_execute_callback execute;
  napi_async_complete_callback complete;
};

static napi_async_work async_work_init(
  napi_env env,
  napi_value async_resource,
  napi_value async_resource_name,
  napi_async_execute_callback execute,
  napi_async_complete_callback complete,
  void* data
) {
  napi_async_work work = (napi_async_work)calloc(1, sizeof(struct napi_async_work__));
  if (work == NULL) return NULL;
  EMNAPI_ASYNC_RESOURCE_CTOR(env, async_resource, async_resource_name, (emnapi_async_resource*)work);
  work->env = env;
  work->execute = execute;
  work->complete = complete;
  work->data = data;
  return work;
}

static void async_work_delete(napi_async_work work) {
  EMNAPI_ASYNC_RESOURCE_DTOR(work->env, (emnapi_async_resource*)work);
  free(work);
}

static void async_work_do_thread_pool_work(napi_async_work work) {
  work->execute(work->env, work->data);
}

typedef struct complete_wrap_s {
  int status;
  napi_async_work work;
} complete_wrap_t;

static napi_status convert_error_code(int code) {
  switch (code) {
    case 0:
      return napi_ok;
    case EINVAL:
      return napi_invalid_arg;
    case ECANCELED:
      return napi_cancelled;
    default:
      return napi_generic_failure;
  }
}

static void async_work_on_complete(napi_env env, void* args) {
  complete_wrap_t* wrap = (complete_wrap_t*) args;
  napi_status status = convert_error_code(wrap->status);
  napi_async_work work = wrap->work;
  free(wrap);
  napi_env _env = work->env;
  void* data = work->data;
  work->complete(_env, status, data);
}

static napi_value async_work_after_cb(napi_env env, napi_callback_info info) {
  void* data = NULL;
  EMNAPI_ASSERT_CALL(napi_get_cb_info(env, info, NULL, NULL, NULL, &data));
  complete_wrap_t* wrap = (complete_wrap_t*) data;
  _emnapi_callback_into_module(1, env, async_work_on_complete, wrap, 1);
  return NULL;
}

static void async_work_after_thread_pool_work(napi_async_work work, int status) {
  if (work->complete == NULL) return;
  napi_handle_scope scope;
  napi_value resource, cb;
  napi_env env = work->env;
  EMNAPI_ASSERT_CALL(napi_open_handle_scope(env, &scope));
  EMNAPI_ASSERT_CALL(napi_get_reference_value(env, work->resource_, &resource));
  complete_wrap_t* wrap = (complete_wrap_t*) malloc(sizeof(complete_wrap_t));
  assert(wrap != NULL);
  wrap->status = status;
  wrap->work = work;
  if (emnapi_is_node_binding_available()) {
    EMNAPI_ASSERT_CALL(napi_create_function(env, NULL, 0, async_work_after_cb, wrap, &cb));
    _emnapi_node_make_callback(env,
                              resource,
                              cb,
                              NULL,
                              0,
                              work->async_context_.async_id,
                              work->async_context_.trigger_async_id,
                              NULL);
  } else {
    _emnapi_callback_into_module(1, env, async_work_on_complete, wrap, 1);
  }
  EMNAPI_ASSERT_CALL(napi_close_handle_scope(env, scope));
}

static void async_work_schedule_work_on_execute(uv_work_t* req) {
  napi_async_work self = container_of(req, struct napi_async_work__, work_req_);
  async_work_do_thread_pool_work(self);
}

static void async_work_schedule_work_on_complete(uv_work_t* req, int status) {
  napi_async_work self = container_of(req, struct napi_async_work__, work_req_);
  EMNAPI_KEEPALIVE_POP();
  _emnapi_ctx_decrease_waiting_request_counter();
  async_work_after_thread_pool_work(self, status);
}

static void async_work_schedule_work(napi_async_work work) {
  EMNAPI_KEEPALIVE_PUSH();
  _emnapi_ctx_increase_waiting_request_counter();
  int status = uv_queue_work(uv_default_loop(),
                             &work->work_req_,
                             async_work_schedule_work_on_execute,
                             async_work_schedule_work_on_complete);
  CHECK_EQ(status, 0);
}

static int async_work_cancel_work(napi_async_work work) {
  return uv_cancel((uv_req_t*)&work->work_req_);
}

EXTERN_C_END

#endif

EXTERN_C_START

napi_status napi_create_async_work(napi_env env,
                                   napi_value async_resource,
                                   napi_value async_resource_name,
                                   napi_async_execute_callback execute,
                                   napi_async_complete_callback complete,
                                   void* data,
                                   napi_async_work* result) {
#if EMNAPI_HAVE_THREADS
  CHECK_ENV_NOT_IN_GC(env);
  CHECK_ARG(env, execute);
  CHECK_ARG(env, result);

  napi_status status;
  napi_value resource;
  napi_value resource_name;
  if (async_resource != NULL) {
    status = napi_coerce_to_object(env, async_resource, &resource);
    if (status != napi_ok) return status;
  } else {
    status = napi_create_object(env, &resource);
    if (status != napi_ok) return status;
  }

  CHECK_ARG(env, async_resource_name);
  status = napi_coerce_to_string(env, async_resource_name, &resource_name);
  if (status != napi_ok) return status;

  napi_async_work work = async_work_init(env,
                                         resource,
                                         resource_name,
                                         execute,
                                         complete,
                                         data);
  if (work == NULL) {
    return napi_set_last_error(env, napi_generic_failure, 0, NULL);
  }

  *result = work;

  return napi_clear_last_error(env);
#else
  return napi_set_last_error(env, napi_generic_failure, 0, NULL);
#endif
}

napi_status napi_delete_async_work(napi_env env, napi_async_work work) {
#if EMNAPI_HAVE_THREADS
  CHECK_ENV_NOT_IN_GC(env);
  CHECK_ARG(env, work);

  async_work_delete(work);

  return napi_clear_last_error(env);
#else
  return napi_set_last_error(env, napi_generic_failure, 0, NULL);
#endif
}

napi_status napi_queue_async_work(node_api_basic_env env, napi_async_work work) {
#if EMNAPI_HAVE_THREADS
  CHECK_ENV(env);
  CHECK_ARG(env, work);

  async_work_schedule_work(work);

  return napi_clear_last_error(env);
#else
  return napi_set_last_error(env, napi_generic_failure, 0, NULL);
#endif
}

#define CALL_UV(env, condition)                                         \
  do {                                                                  \
    int result = (condition);                                           \
    napi_status status = convert_error_code(result);                    \
    if (status != napi_ok) {                                            \
      return napi_set_last_error(env, status, result, NULL);            \
    }                                                                   \
  } while (0)

napi_status napi_cancel_async_work(node_api_basic_env env, napi_async_work work) {
#if EMNAPI_HAVE_THREADS
  CHECK_ENV(env);
  CHECK_ARG(env, work);

  CALL_UV(env, async_work_cancel_work(work));

  return napi_clear_last_error(env);
#else
  return napi_set_last_error(env, napi_generic_failure, 0, NULL);
#endif
}

EXTERN_C_END
