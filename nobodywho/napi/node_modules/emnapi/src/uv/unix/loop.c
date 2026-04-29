#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include "../uv-common.h"
#include <errno.h>
#include <string.h>

int _emnapi_create_proxying_queue(uv_loop_t* loop);
void _emnapi_destroy_proxying_queue(uv_loop_t* loop);

int uv_loop_init(uv_loop_t* loop) {
  uv__loop_internal_fields_t* lfields;
  void* saved_data;
  int err;

  saved_data = loop->data;
  memset(loop, 0, sizeof(*loop));
  loop->data = saved_data;

  lfields = (uv__loop_internal_fields_t*) uv__calloc(1, sizeof(*lfields));
  if (lfields == NULL)
    return ENOMEM;
  loop->internal_fields = lfields;

  err = uv_mutex_init(&lfields->loop_metrics.lock);
  if (err)
    goto fail_metrics_mutex_init;
  memset(&lfields->loop_metrics.metrics,
         0,
         sizeof(lfields->loop_metrics.metrics));

  uv__queue_init(&loop->wq);
  uv__queue_init(&loop->async_handles);
  uv__queue_init(&loop->handle_queue);
  err = _emnapi_create_proxying_queue(loop);
  if (err) goto fail_proxying_queue_init;
  err = uv_mutex_init(&loop->wq_mutex);
  if (err) goto fail_mutex_init;
  err = uv_async_init(loop, &loop->wq_async, uv__work_done);
  if (err) goto fail_async_init;
  return 0;

fail_async_init:
  uv_mutex_destroy(&loop->wq_mutex);

fail_mutex_init:

fail_proxying_queue_init:

fail_metrics_mutex_init:
  uv__free(lfields);
  loop->internal_fields = NULL;

  return err;
}

void uv__loop_close(uv_loop_t* loop) {
  uv__loop_internal_fields_t* lfields;

  uv_mutex_lock(&loop->wq_mutex);
  assert(uv__queue_empty(&loop->wq) && "thread pool work queue not empty!");
  assert(!uv__has_active_reqs(loop));
  _emnapi_destroy_proxying_queue(loop);
  uv_mutex_unlock(&loop->wq_mutex);
  uv_mutex_destroy(&loop->wq_mutex);

  lfields = uv__get_internal_fields(loop);
  uv_mutex_destroy(&lfields->loop_metrics.lock);
  uv__free(lfields);
  loop->internal_fields = NULL;
}

#endif
