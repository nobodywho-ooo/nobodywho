#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include <assert.h>
#include <errno.h>
#include <stdlib.h>
#include <time.h>
#include "uv.h"

// #define CHECK_RET(the_call) \
//   do { \
//     int r = (the_call); \
//     if (r) { \
//       fprintf(stderr, #the_call ": %d\n", r); \
//       abort(); \
//     } \
//   } while (0)

static size_t uv__min_stack_size(void) {
  static const size_t min = 8192;

#ifdef PTHREAD_STACK_MIN  /* Not defined on NetBSD. */
  if (min < (size_t) PTHREAD_STACK_MIN)
    return PTHREAD_STACK_MIN;
#endif  /* PTHREAD_STACK_MIN */

  return min;
}

static size_t uv__default_stack_size(void) {
#if !defined(__linux__)
  return 0;
#elif defined(__PPC__) || defined(__ppc__) || defined(__powerpc__)
  return 4 << 20;  /* glibc default. */
#else
  return 2 << 20;  /* glibc default. */
#endif
}

size_t uv__thread_stack_size(void) {
  return uv__default_stack_size();
}

void uv_sem_post(sem_t* sem) {
  if (sem_post(sem))
    abort();
}

int uv_sem_init(sem_t* sem, unsigned int value) {
  if (sem_init(sem, 0, value))
    return errno;
  return 0;
}

void uv_sem_wait(sem_t* sem) {
  int r;

  do {
    r = sem_wait(sem);
  } while (r == -1 && errno == EINTR);

  if (r)
    abort();
}

void uv_sem_destroy(sem_t* sem) {
  if (sem_destroy(sem))
    abort();
}

void uv_once(pthread_once_t* guard, void (*callback)(void)) {
  if (pthread_once(guard, callback))
    abort();
}

int uv_mutex_init(uv_mutex_t* mutex) {
#if defined(__wasi__) || defined(NDEBUG) || !defined(PTHREAD_MUTEX_ERRORCHECK)
  return pthread_mutex_init(mutex, NULL);
#else
  pthread_mutexattr_t attr;
  int err;

  if (pthread_mutexattr_init(&attr))
    abort();

  if (pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_ERRORCHECK))
    abort();

  err = pthread_mutex_init(mutex, &attr);

  if (pthread_mutexattr_destroy(&attr))
    abort();

  return err;
#endif
}

void uv_mutex_destroy(uv_mutex_t* mutex) {
  if (pthread_mutex_destroy(mutex))
    abort();
}

void uv_mutex_lock(uv_mutex_t* mutex) {
  if (pthread_mutex_lock(mutex))
    abort();
}

void uv_mutex_unlock(uv_mutex_t* mutex) {
  if (pthread_mutex_unlock(mutex))
    abort();
}

int uv_cond_init(uv_cond_t* cond) {
  return pthread_cond_init(cond, NULL);
}

void uv_cond_signal(uv_cond_t* cond) {
  if (pthread_cond_signal(cond))
    abort();
}

void uv_cond_wait(uv_cond_t* cond, uv_mutex_t* mutex) {
  if (pthread_cond_wait(cond, mutex))
    abort();
}

void uv_cond_destroy(uv_cond_t* cond) {
  if (pthread_cond_destroy(cond))
    abort();
}

int uv_thread_create_ex(uv_thread_t* tid,
                        const uv_thread_options_t* params,
                        void (*entry)(void *arg),
                        void *arg) {
  int err;
  pthread_attr_t* attr;
  pthread_attr_t attr_storage;
  size_t pagesize;
  size_t stack_size;
  size_t min_stack_size;

  /* Used to squelch a -Wcast-function-type warning. */
  union {
    void (*in)(void*);
    void* (*out)(void*);
  } f;

  stack_size =
      params->flags & UV_THREAD_HAS_STACK_SIZE ? params->stack_size : 0;

  attr = NULL;
  if (stack_size == 0) {
    stack_size = uv__thread_stack_size();
  } else {
    pagesize = 65536;
    stack_size = (stack_size + pagesize - 1) &~ (pagesize - 1);
    min_stack_size = uv__min_stack_size();
    if (stack_size < min_stack_size)
      stack_size = min_stack_size;
  }

  if (stack_size > 0) {
    attr = &attr_storage;

    if (pthread_attr_init(attr))
      abort();

    if (pthread_attr_setstacksize(attr, stack_size))
      abort();
  }

  f.in = entry;
  err = pthread_create(tid, attr, f.out, arg);

  if (attr != NULL)
    pthread_attr_destroy(attr);

  return err;
}

int uv_thread_create(uv_thread_t *tid, void (*entry)(void *arg), void *arg) {
  uv_thread_options_t params;
  params.flags = UV_THREAD_NO_FLAGS;
  return uv_thread_create_ex(tid, &params, entry, arg);
}

int uv_thread_join(uv_thread_t *tid) {
  return pthread_join(*tid, NULL);
}

#endif
