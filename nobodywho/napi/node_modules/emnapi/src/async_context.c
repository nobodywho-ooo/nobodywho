#include <node_api.h>
#include "emnapi_internal.h"

EXTERN_C_START

struct napi_async_context__ {
  int32_t low;
  int32_t high;
};

EMNAPI_INTERNAL_EXTERN napi_status
_emnapi_async_init_js(napi_value async_resource,
                      napi_value async_resource_name,
                      napi_async_context result);
EMNAPI_INTERNAL_EXTERN napi_status
_emnapi_async_destroy_js(napi_async_context async_context);

napi_status
napi_async_init(napi_env env,
                napi_value async_resource,
                napi_value async_resource_name,
                napi_async_context* result) {
  CHECK_ENV_NOT_IN_GC(env);
  CHECK_ARG(env, async_resource_name);
  CHECK_ARG(env, result);

  napi_async_context ret = (napi_async_context) malloc(sizeof(struct napi_async_context__));

  napi_status status = _emnapi_async_init_js(async_resource, async_resource_name, ret);
  if (status != napi_ok) {
    free(ret);
    return napi_set_last_error(env, status, 0, NULL);
  }

  *result = ret;
  return napi_clear_last_error(env);
}

napi_status napi_async_destroy(napi_env env,
                               napi_async_context async_context) {
  CHECK_ENV_NOT_IN_GC(env);
  CHECK_ARG(env, async_context);

  napi_status status = _emnapi_async_destroy_js(async_context);
  if (status != napi_ok) {
    return napi_set_last_error(env, status, 0, NULL);
  }
  free(async_context);

  return napi_clear_last_error(env);
}

EXTERN_C_END
