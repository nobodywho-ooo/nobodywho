#ifndef UV_COMMON_H_
#define UV_COMMON_H_

#if defined(__EMSCRIPTEN__)
  #ifndef EMNAPI_USE_PROXYING
    #include <emscripten.h> /* version.h */
    #if __EMSCRIPTEN_major__ * 10000 + __EMSCRIPTEN_minor__ * 100 + __EMSCRIPTEN_tiny__ >= 30109
    #define EMNAPI_USE_PROXYING 1
    #else
    #define EMNAPI_USE_PROXYING 0
    #endif
  #endif
#else
  #define EMNAPI_USE_PROXYING 0
#endif

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)
#include <assert.h>
#include "uv.h"
#include "queue.h"
#ifndef _MSC_VER
# include <stdatomic.h>
#endif

#define ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
// #define offsetof(s, m) __builtin_offsetof(s, m)
#define container_of(ptr, type, member) \
  ((type *) ((char *) (ptr) - offsetof(type, member)))

/* Handle flags. Some flags are specific to Windows or UNIX. */
enum {
  /* Used by all handles. */
  UV_HANDLE_CLOSING                     = 0x00000001,
  UV_HANDLE_CLOSED                      = 0x00000002,
  UV_HANDLE_ACTIVE                      = 0x00000004,
  UV_HANDLE_REF                         = 0x00000008,
  UV_HANDLE_INTERNAL                    = 0x00000010,
  UV_HANDLE_ENDGAME_QUEUED              = 0x00000020
};

#define uv__has_active_reqs(loop)                                             \
  ((loop)->active_reqs.count > 0)

#define uv__req_register(loop)                                                \
  do {                                                                        \
    (loop)->active_reqs.count++;                                              \
  }                                                                           \
  while (0)

#define uv__req_unregister(loop)                                              \
  do {                                                                        \
    assert(uv__has_active_reqs(loop));                                        \
    (loop)->active_reqs.count--;                                              \
  }                                                                           \
  while (0)

#define uv__has_active_handles(loop)                                          \
  ((loop)->active_handles > 0)

#define uv__active_handle_add(h)                                              \
  do {                                                                        \
    (h)->loop->active_handles++;                                              \
  }                                                                           \
  while (0)

#define uv__active_handle_rm(h)                                               \
  do {                                                                        \
    (h)->loop->active_handles--;                                              \
  }                                                                           \
  while (0)

#define uv__is_active(h)                                                      \
  (((h)->flags & UV_HANDLE_ACTIVE) != 0)

#define uv__is_closing(h)                                                     \
  (((h)->flags & (UV_HANDLE_CLOSING | UV_HANDLE_CLOSED)) != 0)
#define uv__handle_start(h)                                                   \
  do {                                                                        \
    if (((h)->flags & UV_HANDLE_ACTIVE) != 0) break;                          \
    (h)->flags |= UV_HANDLE_ACTIVE;                                           \
    if (((h)->flags & UV_HANDLE_REF) != 0) uv__active_handle_add(h);          \
  }                                                                           \
  while (0)

#define uv__handle_stop(h)                                                    \
  do {                                                                        \
    if (((h)->flags & UV_HANDLE_ACTIVE) == 0) break;                          \
    (h)->flags &= ~UV_HANDLE_ACTIVE;                                          \
    if (((h)->flags & UV_HANDLE_REF) != 0) uv__active_handle_rm(h);           \
  }                                                                           \
  while (0)

#define uv__handle_ref(h)                                                     \
  do {                                                                        \
    if (((h)->flags & UV_HANDLE_REF) != 0) break;                             \
    (h)->flags |= UV_HANDLE_REF;                                              \
    if (((h)->flags & UV_HANDLE_CLOSING) != 0) break;                         \
    if (((h)->flags & UV_HANDLE_ACTIVE) != 0) uv__active_handle_add(h);       \
  }                                                                           \
  while (0)

#define uv__handle_unref(h)                                                   \
  do {                                                                        \
    if (((h)->flags & UV_HANDLE_REF) == 0) break;                             \
    (h)->flags &= ~UV_HANDLE_REF;                                             \
    if (((h)->flags & UV_HANDLE_CLOSING) != 0) break;                         \
    if (((h)->flags & UV_HANDLE_ACTIVE) != 0) uv__active_handle_rm(h);        \
  }                                                                           \
  while (0)

#define uv__has_ref(h)                                                        \
  (((h)->flags & UV_HANDLE_REF) != 0)

#if defined(_WIN32)
# define uv__handle_platform_init(h) ((h)->u.fd = -1)
#else
# define uv__handle_platform_init(h) ((h)->next_closing = NULL)
#endif

#define uv__handle_init(loop_, h, type_)                                      \
  do {                                                                        \
    (h)->loop = (loop_);                                                      \
    (h)->type = (type_);                                                      \
    (h)->flags = UV_HANDLE_REF;  /* Ref the loop when active. */              \
    uv__queue_insert_tail(&(loop_)->handle_queue, &(h)->handle_queue);        \
    uv__handle_platform_init(h);                                              \
  }                                                                           \
  while (0)

#define UV_REQ_INIT(req, typ)                                                 \
  do {                                                                        \
    (req)->type = (typ);                                                      \
  }                                                                           \
  while (0)

#define uv__req_init(loop, req, typ)                                          \
  do {                                                                        \
    UV_REQ_INIT(req, typ);                                                    \
    uv__req_register(loop);                                                   \
  }                                                                           \
  while (0)

#define uv__get_internal_fields(loop)                                         \
  ((uv__loop_internal_fields_t*) loop->internal_fields)

#define uv__get_loop_metrics(loop)                                            \
  (&uv__get_internal_fields(loop)->loop_metrics)

#define uv__metrics_inc_loop_count(loop)                                      \
  do {                                                                        \
    uv__get_loop_metrics(loop)->metrics.loop_count++;                         \
  } while (0)

#define uv__metrics_inc_events(loop, e)                                       \
  do {                                                                        \
    uv__get_loop_metrics(loop)->metrics.events += (e);                        \
  } while (0)

#define uv__metrics_inc_events_waiting(loop, e)                               \
  do {                                                                        \
    uv__get_loop_metrics(loop)->metrics.events_waiting += (e);                \
  } while (0)

#define uv__exchange_int_relaxed(p, v)                                        \
  atomic_exchange_explicit((_Atomic int*)(p), v, memory_order_relaxed)

enum uv__work_kind {
  UV__WORK_CPU,
  UV__WORK_FAST_IO,
  UV__WORK_SLOW_IO
};

void uv__threadpool_cleanup(void);
void uv__work_done(uv_async_t* handle);
void uv__loop_close(uv_loop_t* loop);

void *uv__calloc(size_t count, size_t size);
char *uv__strdup(const char* s);
char *uv__strndup(const char* s, size_t n);
void* uv__malloc(size_t size);
void uv__free(void* ptr);
void* uv__realloc(void* ptr, size_t size);
void* uv__reallocf(void* ptr, size_t size);

typedef struct uv__loop_metrics_s uv__loop_metrics_t;
typedef struct uv__loop_internal_fields_s uv__loop_internal_fields_t;

struct uv__loop_metrics_s {
  uv_metrics_t metrics;
  uint64_t provider_entry_time;
  uint64_t provider_idle_time;
  uv_mutex_t lock;
};

struct uv__loop_internal_fields_s {
  unsigned int flags;
  uv__loop_metrics_t loop_metrics;
  int current_timeout;
};

#endif

#endif /* UV_COMMON_H_ */
