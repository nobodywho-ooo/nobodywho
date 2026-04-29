#ifndef UV_UNIX_H
#define UV_UNIX_H

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include <semaphore.h>
#include <pthread.h>

#include "threadpool.h"

#define UV_ONCE_INIT PTHREAD_ONCE_INIT

typedef pthread_once_t uv_once_t;
typedef pthread_t uv_thread_t;
typedef pthread_mutex_t uv_mutex_t;
typedef sem_t uv_sem_t;
typedef pthread_cond_t uv_cond_t;

#endif

#define UV_HANDLE_PRIVATE_FIELDS                                              \
  uv_handle_t* next_closing;                                                  \
  unsigned int flags;                                                         \

#endif /* UV_UNIX_H */
