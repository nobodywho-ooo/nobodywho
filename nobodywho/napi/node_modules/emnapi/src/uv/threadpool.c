/* Copyright Joyent, Inc. and other Node contributors. All rights reserved.
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to
 * deal in the Software without restriction, including without limitation the
 * rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
 * sell copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
 * IN THE SOFTWARE.
 */

// from libuv 1.48.0

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include <stdlib.h>
#include <errno.h>
#include <string.h>
#include "uv-common.h"
#include "emnapi_common.h"

#if defined(__wasi__) && defined(_REENTRANT)
#define __EMNAPI_WASI_THREADS__
#endif

#define MAX_THREADPOOL_SIZE 1024

static uv_once_t once = UV_ONCE_INIT;
static uv_cond_t cond;
static uv_mutex_t mutex;
static unsigned int idle_threads;
static unsigned int slow_io_work_running;
static unsigned int nthreads;
static uv_thread_t* threads;
static uv_thread_t default_threads[4];
static struct uv__queue exit_message;
static struct uv__queue wq;
static struct uv__queue run_slow_work_message;
static struct uv__queue slow_io_pending_wq;

static unsigned int slow_work_thread_threshold(void) {
  return (nthreads + 1) / 2;
}

static void uv__cancelled(struct uv__work* w) {
  abort();
}

EMNAPI_INTERNAL_EXTERN void _emnapi_worker_unref(uv_thread_t pid);

#ifdef __EMNAPI_WASI_THREADS__
EMNAPI_INTERNAL_EXTERN
void _emnapi_after_uvthreadpool_ready(void (*callback)(struct uv__queue* w, enum uv__work_kind kind),
                                      struct uv__queue* w,
                                      enum uv__work_kind kind);
EMNAPI_INTERNAL_EXTERN void _emnapi_tell_js_uvthreadpool(uv_thread_t* threads, unsigned int n);
EMNAPI_INTERNAL_EXTERN void _emnapi_emit_async_thread_ready();
#endif

/* To avoid deadlock with uv_cancel() it's crucial that the worker
 * never holds the global mutex and the loop-local mutex at the same time.
 */
static void* worker(void* arg) {
  struct uv__work* w;
  struct uv__queue* q;
  int is_slow_work;
#ifndef __EMNAPI_WASI_THREADS__
  uv_sem_post((uv_sem_t*) arg);
#else
  _emnapi_emit_async_thread_ready();
#endif
  arg = NULL;

  uv_mutex_lock(&mutex);
  for (;;) {
    /* `mutex` should always be locked at this point. */

    /* Keep waiting while either no work is present or only slow I/O
       and we're at the threshold for that. */
    while (uv__queue_empty(&wq) ||
           (uv__queue_head(&wq) == &run_slow_work_message &&
            uv__queue_next(&run_slow_work_message) == &wq &&
            slow_io_work_running >= slow_work_thread_threshold())) {
      idle_threads += 1;
      uv_cond_wait(&cond, &mutex);
      idle_threads -= 1;
    }

    q = uv__queue_head(&wq);
    if (q == &exit_message) {
      uv_cond_signal(&cond);
      uv_mutex_unlock(&mutex);
      break;
    }

    uv__queue_remove(q);
    uv__queue_init(q);  /* Signal uv_cancel() that the work req is executing. */

    is_slow_work = 0;
    if (q == &run_slow_work_message) {
      /* If we're at the slow I/O threshold, re-schedule until after all
         other work in the queue is done. */
      if (slow_io_work_running >= slow_work_thread_threshold()) {
        uv__queue_insert_tail(&wq, q);
        continue;
      }

      /* If we encountered a request to run slow I/O work but there is none
         to run, that means it's cancelled => Start over. */
      if (uv__queue_empty(&slow_io_pending_wq))
        continue;

      is_slow_work = 1;
      slow_io_work_running++;

      q = uv__queue_head(&slow_io_pending_wq);
      uv__queue_remove(q);
      uv__queue_init(q);

      /* If there is more slow I/O work, schedule it to be run as well. */
      if (!uv__queue_empty(&slow_io_pending_wq)) {
        uv__queue_insert_tail(&wq, &run_slow_work_message);
        if (idle_threads > 0)
          uv_cond_signal(&cond);
      }
    }

    uv_mutex_unlock(&mutex);

    w = uv__queue_data(q, struct uv__work, wq);
    w->work(w);

    uv_mutex_lock(&w->loop->wq_mutex);
    w->work = NULL;  /* Signal uv_cancel() that the work req is done
                        executing. */
    uv__queue_insert_tail(&w->loop->wq, &w->wq);
    uv_async_send(&w->loop->wq_async);
    uv_mutex_unlock(&w->loop->wq_mutex);

    /* Lock `mutex` since that is expected at the start of the next
     * iteration. */
    uv_mutex_lock(&mutex);
    if (is_slow_work) {
      /* `slow_io_work_running` is protected by `mutex`. */
      slow_io_work_running--;
    }
  }
  return NULL;
}


static void post(struct uv__queue* q, enum uv__work_kind kind) {
  uv_mutex_lock(&mutex);
  // if (kind == UV__WORK_SLOW_IO) {
  //   /* Insert into a separate queue. */
  //   uv__queue_insert_tail(&slow_io_pending_wq, q);
  //   if (!uv__queue_empty(&run_slow_work_message)) {
  //     /* Running slow I/O tasks is already scheduled => Nothing to do here.
  //        The worker that runs said other task will schedule this one as well. */
  //     uv_mutex_unlock(&mutex);
  //     return;
  //   }
  //   q = &run_slow_work_message;
  // }

  uv__queue_insert_tail(&wq, q);
  if (idle_threads > 0)
    uv_cond_signal(&cond);
  uv_mutex_unlock(&mutex);
}


#ifdef __MVS__
/* TODO(itodorov) - zos: revisit when Woz compiler is available. */
__attribute__((destructor))
#endif
void uv__threadpool_cleanup(void) {
  unsigned int i;

  if (nthreads == 0)
    return;

#ifndef __MVS__
  /* TODO(gabylb) - zos: revisit when Woz compiler is available. */
  post(&exit_message, UV__WORK_CPU);
#endif

  for (i = 0; i < nthreads; i++)
    if (uv_thread_join(threads + i))
      abort();

  if (threads != default_threads)
    free(threads);

  uv_mutex_destroy(&mutex);
  uv_cond_destroy(&cond);

  threads = NULL;
  nthreads = 0;
}

EMNAPI_INTERNAL_EXTERN int _emnapi_async_work_pool_size();

static void init_threads(void) {
  uv_thread_options_t config;
  unsigned int i;
#if !defined(EMNAPI_WORKER_POOL_SIZE) || !(EMNAPI_WORKER_POOL_SIZE > 0)
  const char* val;
#endif
#ifndef __EMNAPI_WASI_THREADS__
  uv_sem_t sem;
#endif

#if defined(EMNAPI_WORKER_POOL_SIZE) && EMNAPI_WORKER_POOL_SIZE > 0
  nthreads = EMNAPI_WORKER_POOL_SIZE;
#else
  nthreads = ARRAY_SIZE(default_threads);
  int async_work_pool_size = _emnapi_async_work_pool_size();
  if (async_work_pool_size > 0) {
    nthreads = (unsigned int) async_work_pool_size;
  } else {
    val = getenv("UV_THREADPOOL_SIZE");
    if (val != NULL)
      nthreads = atoi(val);
  }
#endif
  if (nthreads == 0)
    nthreads = 1;
  if (nthreads > MAX_THREADPOOL_SIZE)
    nthreads = MAX_THREADPOOL_SIZE;

  threads = default_threads;
  if (nthreads > ARRAY_SIZE(default_threads)) {
    threads = (uv_thread_t *)malloc(nthreads * sizeof(threads[0]));
    if (threads == NULL) {
      nthreads = ARRAY_SIZE(default_threads);
      threads = default_threads;
    }
  }

  if (uv_cond_init(&cond))
    abort();

  if (uv_mutex_init(&mutex))
    abort();

  uv__queue_init(&wq);
  uv__queue_init(&slow_io_pending_wq);
  uv__queue_init(&run_slow_work_message);

#ifndef __EMNAPI_WASI_THREADS__
  if (uv_sem_init(&sem, 0))
    abort();
#endif

  // config.flags = UV_THREAD_HAS_STACK_SIZE;
  // config.stack_size = 8u << 20;  /* 8 MB */

  // use DEFAULT_PTHREAD_STACK_SIZE
  config.flags = UV_THREAD_NO_FLAGS;

  for (i = 0; i < nthreads; i++)
#ifndef __EMNAPI_WASI_THREADS__
    if (uv_thread_create_ex(threads + i, &config, (uv_thread_cb) worker, &sem))
#else
    if (uv_thread_create_ex(threads + i, &config, (uv_thread_cb) worker, NULL))
#endif
      abort();

#ifndef __EMNAPI_WASI_THREADS__
  for (i = 0; i < nthreads; i++)
    uv_sem_wait(&sem);

  uv_sem_destroy(&sem);
  for (i = 0; i < nthreads; i++)
    _emnapi_worker_unref(*(threads + i));
#else
  _emnapi_tell_js_uvthreadpool(threads, nthreads);
#endif
}


#if !defined(_WIN32) && !defined(__wasi__)
static void reset_once(void) {
  uv_once_t child_once = UV_ONCE_INIT;
  memcpy(&once, &child_once, sizeof(child_once));
}
#endif


static void init_once(void) {
#if !defined(_WIN32) && !defined(__wasi__)
  /* Re-initialize the threadpool after fork.
   * Note that this discards the global mutex and condition as well
   * as the work queue.
   */
  if (pthread_atfork(NULL, NULL, &reset_once))
    abort();
#endif
  init_threads();
}


void uv__work_submit(uv_loop_t* loop,
                     struct uv__work* w,
                     enum uv__work_kind kind,
                     void (*work)(struct uv__work* w),
                     void (*done)(struct uv__work* w, int status)) {
  uv_once(&once, init_once);
  w->loop = loop;
  w->work = work;
  w->done = done;
// #ifdef __EMNAPI_WASI_THREADS__
//   _emnapi_after_uvthreadpool_ready(post, &w->wq, kind);
// #else
  post(&w->wq, kind);
// #endif
}


static int uv__work_cancel(uv_loop_t* loop, uv_req_t* req, struct uv__work* w) {
  int cancelled;

  uv_once(&once, init_once);  /* Ensure |mutex| is initialized. */
  uv_mutex_lock(&mutex);
  uv_mutex_lock(&w->loop->wq_mutex);

  cancelled = !uv__queue_empty(&w->wq) && w->work != NULL;
  if (cancelled)
    uv__queue_remove(&w->wq);

  uv_mutex_unlock(&w->loop->wq_mutex);
  uv_mutex_unlock(&mutex);

  if (!cancelled)
    return EBUSY;

  w->work = uv__cancelled;
  uv_mutex_lock(&loop->wq_mutex);
  uv__queue_insert_tail(&loop->wq, &w->wq);
  uv_async_send(&loop->wq_async);
  uv_mutex_unlock(&loop->wq_mutex);

  return 0;
}


void uv__work_done(uv_async_t* handle) {
  struct uv__work* w;
  uv_loop_t* loop;
  struct uv__queue* q;
  struct uv__queue wq;
  int err;
  int nevents;

  loop = container_of(handle, uv_loop_t, wq_async);
  uv_mutex_lock(&loop->wq_mutex);
  uv__queue_move(&loop->wq, &wq);
  uv_mutex_unlock(&loop->wq_mutex);

  nevents = 0;

  while (!uv__queue_empty(&wq)) {
    q = uv__queue_head(&wq);
    uv__queue_remove(q);

    w = container_of(q, struct uv__work, wq);
    err = (w->work == uv__cancelled) ? ECANCELED : 0;
    w->done(w, err);
    nevents++;
  }

  /* This check accomplishes 2 things:
   * 1. Even if the queue was empty, the call to uv__work_done() should count
   *    as an event. Which will have been added by the event loop when
   *    calling this callback.
   * 2. Prevents accidental wrap around in case nevents == 0 events == 0.
   */
  if (nevents > 1) {
    /* Subtract 1 to counter the call to uv__work_done(). */
    uv__metrics_inc_events(loop, nevents - 1);
    if (uv__get_internal_fields(loop)->current_timeout == 0)
      uv__metrics_inc_events_waiting(loop, nevents - 1);
  }
}


static void uv__queue_work(struct uv__work* w) {
  uv_work_t* req = container_of(w, uv_work_t, work_req);

  req->work_cb(req);
}


static void uv__queue_done(struct uv__work* w, int err) {
  uv_work_t* req;

  req = container_of(w, uv_work_t, work_req);
  uv__req_unregister(req->loop);

  if (req->after_work_cb == NULL)
    return;

  req->after_work_cb(req, err);
}


int uv_queue_work(uv_loop_t* loop,
                  uv_work_t* req,
                  uv_work_cb work_cb,
                  uv_after_work_cb after_work_cb) {
  if (work_cb == NULL)
    return EINVAL;

  uv__req_init(loop, req, UV_WORK);
  req->loop = loop;
  req->work_cb = work_cb;
  req->after_work_cb = after_work_cb;
  uv__work_submit(loop,
                  &req->work_req,
                  UV__WORK_CPU,
                  uv__queue_work,
                  uv__queue_done);
  return 0;
}


int uv_cancel(uv_req_t* req) {
  struct uv__work* wreq;
  uv_loop_t* loop;

  switch (req->type) {
  // case UV_FS:
  //   loop =  ((uv_fs_t*) req)->loop;
  //   wreq = &((uv_fs_t*) req)->work_req;
  //   break;
  // case UV_GETADDRINFO:
  //   loop =  ((uv_getaddrinfo_t*) req)->loop;
  //   wreq = &((uv_getaddrinfo_t*) req)->work_req;
  //   break;
  // case UV_GETNAMEINFO:
  //   loop = ((uv_getnameinfo_t*) req)->loop;
  //   wreq = &((uv_getnameinfo_t*) req)->work_req;
  //   break;
  // case UV_RANDOM:
  //   loop = ((uv_random_t*) req)->loop;
  //   wreq = &((uv_random_t*) req)->work_req;
  //   break;
  case UV_WORK:
    loop =  ((uv_work_t*) req)->loop;
    wreq = &((uv_work_t*) req)->work_req;
    break;
  default:
    return EINVAL;
  }

  return uv__work_cancel(loop, req, wreq);
}

#endif
