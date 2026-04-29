#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include "uv.h"
#include "internal.h"
#include <errno.h>
#include <time.h>

uint64_t uv_hrtime(void) {
  return uv__hrtime(UV_CLOCK_PRECISE);
}

void uv_close(uv_handle_t* handle, uv_close_cb close_cb) {
  assert(!uv__is_closing(handle));

  handle->flags |= UV_HANDLE_CLOSING;
  handle->close_cb = close_cb;

  switch (handle->type) {
  case UV_ASYNC:
    uv__async_close((uv_async_t*)handle);
    break;
  default:
    assert(0);
  }

  uv__make_close_pending(handle);
}

static void uv__finish_close(uv_handle_t* handle) {
  assert(handle->flags & UV_HANDLE_CLOSING);
  assert(!(handle->flags & UV_HANDLE_CLOSED));
  handle->flags |= UV_HANDLE_CLOSED;

  uv__handle_unref(handle);
  uv__queue_remove(&handle->handle_queue);

  if (handle->close_cb) {
    handle->close_cb(handle);
  }
}

void uv__make_close_pending(uv_handle_t* handle) {
  assert(handle->flags & UV_HANDLE_CLOSING);
  assert(!(handle->flags & UV_HANDLE_CLOSED));
  // handle->next_closing = handle->loop->closing_handles;
  // handle->loop->closing_handles = handle;
  NEXT_TICK(((void (*)(void *))uv__finish_close), handle);
}

int uv_is_closing(const uv_handle_t* handle) {
  return uv__is_closing(handle);
}

int nanosleep(const struct timespec *, struct timespec *);

void uv_sleep(unsigned int msec) {
  struct timespec timeout;
  int rc;

  timeout.tv_sec = msec / 1000;
  timeout.tv_nsec = (msec % 1000) * 1000 * 1000;

  do
    rc = nanosleep(&timeout, &timeout);
  while (rc == -1 && errno == EINTR);

  assert(rc == 0);
}

#endif
