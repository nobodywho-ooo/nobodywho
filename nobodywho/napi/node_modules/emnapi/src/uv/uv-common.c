#include <stddef.h>

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include "uv-common.h"

typedef struct {
  uv_malloc_func local_malloc;
  uv_realloc_func local_realloc;
  uv_calloc_func local_calloc;
  uv_free_func local_free;
} uv__allocator_t;

static uv__allocator_t uv__allocator = {
  malloc,
  realloc,
  calloc,
  free,
};

char* uv__strdup(const char* s) {
  size_t len = strlen(s) + 1;
  char* m = uv__malloc(len);
  if (m == NULL)
    return NULL;
  return memcpy(m, s, len);
}

char* uv__strndup(const char* s, size_t n) {
  char* m;
  size_t len = strlen(s);
  if (n < len)
    len = n;
  m = uv__malloc(len + 1);
  if (m == NULL)
    return NULL;
  m[len] = '\0';
  return memcpy(m, s, len);
}

void* uv__malloc(size_t size) {
  if (size > 0)
    return uv__allocator.local_malloc(size);
  return NULL;
}

void uv__free(void* ptr) {
  int saved_errno;

  /* Libuv expects that free() does not clobber errno.  The system allocator
   * honors that assumption but custom allocators may not be so careful.
   */
  saved_errno = errno;
  uv__allocator.local_free(ptr);
  errno = saved_errno;
}

void* uv__calloc(size_t count, size_t size) {
  return uv__allocator.local_calloc(count, size);
}

void* uv__realloc(void* ptr, size_t size) {
  if (size > 0)
    return uv__allocator.local_realloc(ptr, size);
  uv__free(ptr);
  return NULL;
}

void* uv__reallocf(void* ptr, size_t size) {
  void* newptr;

  newptr = uv__realloc(ptr, size);
  if (newptr == NULL)
    if (size > 0)
      uv__free(ptr);

  return newptr;
}

int uv_replace_allocator(uv_malloc_func malloc_func,
                         uv_realloc_func realloc_func,
                         uv_calloc_func calloc_func,
                         uv_free_func free_func) {
  if (malloc_func == NULL || realloc_func == NULL ||
      calloc_func == NULL || free_func == NULL) {
    return EINVAL;
  }

  uv__allocator.local_malloc = malloc_func;
  uv__allocator.local_realloc = realloc_func;
  uv__allocator.local_calloc = calloc_func;
  uv__allocator.local_free = free_func;

  return 0;
}

static uv_loop_t default_loop_struct;
static uv_loop_t* default_loop_ptr;


uv_loop_t* uv_default_loop(void) {
  if (default_loop_ptr != NULL)
    return default_loop_ptr;

  if (uv_loop_init(&default_loop_struct))
    return NULL;

  default_loop_ptr = &default_loop_struct;
  return default_loop_ptr;
}

int uv_loop_close(uv_loop_t* loop) {
  struct uv__queue* q;
  uv_handle_t* h;
#ifndef NDEBUG
  void* saved_data;
#endif

  if (uv__has_active_reqs(loop))
    return EBUSY;

  uv__queue_foreach(q, &loop->handle_queue) {
    h = uv__queue_data(q, uv_handle_t, handle_queue);
    if (!(h->flags & UV_HANDLE_INTERNAL))
      return EBUSY;
  }

  uv__loop_close(loop);

#ifndef NDEBUG
  saved_data = loop->data;
  memset(loop, -1, sizeof(*loop));
  loop->data = saved_data;
#endif
  if (loop == default_loop_ptr)
    default_loop_ptr = NULL;

  return 0;
}

#if defined(__GNUC__) && !defined(_WIN32)
__attribute__((destructor))
#endif
void uv_library_shutdown(void) {
  static int was_shutdown;

  if (uv__exchange_int_relaxed(&was_shutdown, 1))
    return;

//   uv__process_title_cleanup();
//   uv__signal_cleanup();
// #ifdef __MVS__
//   /* TODO(itodorov) - zos: revisit when Woz compiler is available. */
//   uv__os390_cleanup();
// #else
  uv__threadpool_cleanup();
// #endif
}

int uv_metrics_info(uv_loop_t* loop, uv_metrics_t* metrics) {
  memcpy(metrics,
         &uv__get_loop_metrics(loop)->metrics,
         sizeof(*metrics));

  return 0;
}

uint64_t uv_metrics_idle_time(uv_loop_t* loop) {
  uv__loop_metrics_t* loop_metrics;
  uint64_t entry_time;
  uint64_t idle_time;

  loop_metrics = uv__get_loop_metrics(loop);
  uv_mutex_lock(&loop_metrics->lock);
  idle_time = loop_metrics->provider_idle_time;
  entry_time = loop_metrics->provider_entry_time;
  uv_mutex_unlock(&loop_metrics->lock);

  if (entry_time > 0)
    idle_time += uv_hrtime() - entry_time;
  return idle_time;
}

#else

typedef struct uv_loop_s uv_loop_t;
uv_loop_t* uv_default_loop(void) {
  return NULL;
}

#endif
