/* Copyright Joyent, Inc. and other Node contributors. All rights reserved.
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

/* This file contains both the uv__async internal infrastructure and the
 * user-facing uv_async_t functions.
 */

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)

#include "uv.h"
#include "internal.h"

#include <stdatomic.h>
#include <stdlib.h>
#include <sched.h>
#include "emnapi_common.h"

#if EMNAPI_USE_PROXYING
#include <emscripten/threading.h>
#include <emscripten/proxying.h>
#include <errno.h>

int _emnapi_create_proxying_queue(uv_loop_t* loop) {
  em_proxying_queue* queue = em_proxying_queue_create();
  loop->em_queue = queue;
  if (queue == NULL) return ENOMEM;
  return 0;
}

void _emnapi_destroy_proxying_queue(uv_loop_t* loop) {
  if (loop->em_queue != NULL) {
    em_proxying_queue_destroy(loop->em_queue);
  }
}

#else
EMNAPI_INTERNAL_EXTERN void _emnapi_async_send_js(int type,
                                  void (*callback)(void*),
                                  void* data);

int _emnapi_create_proxying_queue(uv_loop_t* loop) {
  loop->em_queue = NULL;
  return 0;
}

void _emnapi_destroy_proxying_queue(uv_loop_t* loop) {}

#endif

static void uv__async_send(uv_loop_t* loop);
static void uv__cpu_relax(void);

int uv_async_init(uv_loop_t* loop, uv_async_t* handle, uv_async_cb async_cb) {
  uv__handle_init(loop, (uv_handle_t*)handle, UV_ASYNC);
  handle->async_cb = async_cb;
  handle->pending = 0;
  handle->u.fd = 0; /* This will be used as a busy flag. */

  uv__queue_insert_tail(&loop->async_handles, &handle->queue);
  uv__handle_start(handle);
  return 0;
}

/* Only call this from the event loop thread. */
static void uv__async_spin(uv_async_t* handle) {
  _Atomic int* pending;
  _Atomic int* busy;
  int i;

  pending = (_Atomic int*) &handle->pending;
  busy = (_Atomic int*) &handle->u.fd;

  /* Set the pending flag first, so no new events will be added by other
   * threads after this function returns. */
  atomic_store(pending, 1);

  for (;;) {
    /* 997 is not completely chosen at random. It's a prime number, acyclic by
     * nature, and should therefore hopefully dampen sympathetic resonance.
     */
    for (i = 0; i < 997; i++) {
      if (atomic_load(busy) == 0)
        return;

      /* Other thread is busy with this handle, spin until it's done. */
      uv__cpu_relax();
    }

    /* Yield the CPU. We may have preempted the other thread while it's
     * inside the critical section and if it's running on the same CPU
     * as us, we'll just burn CPU cycles until the end of our time slice.
     */
    sched_yield();
  }
}

static void uv__async_io(uv_loop_t* loop) {
  struct uv__queue queue;
  struct uv__queue* q;
  uv_async_t* h;
  _Atomic int *pending;

  uv__queue_move(&loop->async_handles, &queue);
  while (!uv__queue_empty(&queue)) {
    q = uv__queue_head(&queue);
    h = uv__queue_data(q, uv_async_t, queue);

    uv__queue_remove(q);
    uv__queue_insert_tail(&loop->async_handles, q);

    /* Atomically fetch and clear pending flag */
    pending = (_Atomic int*) &h->pending;
    if (atomic_exchange(pending, 0) == 0)
      continue;

    if (h->async_cb == NULL)
      continue;

    h->async_cb(h);
  }
}

#if EMNAPI_USE_PROXYING

#undef emscripten_main_browser_thread_id

__attribute__((weak))
pthread_t emscripten_main_browser_thread_id(void) {
  return NULL;
}

__attribute__((weak))
pthread_t emscripten_main_runtime_thread_id(void) {
  return emscripten_main_browser_thread_id();
}
#endif

static void uv__async_send(uv_loop_t* loop) {
#if EMNAPI_USE_PROXYING
  pthread_t main_thread = emscripten_main_runtime_thread_id();
  assert(main_thread != NULL);
  if (pthread_equal(main_thread, pthread_self())) {
    NEXT_TICK((void (*)(void *))uv__async_io, loop);
  } else {
    // Neither emscripten_dispatch_to_thread_async nor MAIN_THREAD_ASYNC_EM_ASM
    // invoke the async_cb callback if there is a printf() in worker thread.
    // Using emscripten_proxy_async(emscripten_proxy_get_system_queue(), ...)
    // also has the same problem. Not sure what happens.
    // But creating a new queue is all ok.
    if (!emscripten_proxy_async(loop->em_queue,
                                main_thread,
                                (void (*)(void *))uv__async_io,
                                loop)) {
      abort();
    }
  }
#else
  _emnapi_async_send_js(EMNAPI_NEXTTICK_TYPE,
                        (void (*)(void *))uv__async_io,
                        loop);
#endif
}

#define ACCESS_ONCE(type, var) (*(volatile type*) &(var))

int uv_async_send(uv_async_t* handle) {
  _Atomic int* pending;
  _Atomic int* busy;

  pending = (_Atomic int*) &handle->pending;
  busy = (_Atomic int*) &handle->u.fd;

  /* Do a cheap read first. */
  if (atomic_load_explicit(pending, memory_order_relaxed) != 0)
    return 0;

  /* Set the loop to busy. */
  atomic_fetch_add(busy, 1);

  /* Wake up the other thread's event loop. */
  if (atomic_exchange(pending, 1) == 0)
    uv__async_send(handle->loop);

  /* Set the loop to not-busy. */
  atomic_fetch_add(busy, -1);

  return 0;
}

void uv__async_close(uv_async_t* handle) {
  uv__async_spin(handle);
  uv__queue_remove(&handle->queue);
  uv__handle_stop(handle);
}

static void uv__cpu_relax(void) {
#if defined(__i386__) || defined(__x86_64__)
  __asm__ __volatile__ ("rep; nop" ::: "memory");  /* a.k.a. PAUSE */
#elif (defined(__arm__) && __ARM_ARCH >= 7) || defined(__aarch64__)
  __asm__ __volatile__ ("yield" ::: "memory");
#elif (defined(__ppc__) || defined(__ppc64__)) && defined(__APPLE__)
  __asm volatile ("" : : : "memory");
#elif !defined(__APPLE__) && (defined(__powerpc64__) || defined(__ppc64__) || defined(__PPC64__))
  __asm__ __volatile__ ("or 1,1,1; or 2,2,2" ::: "memory");
#endif
}

#endif
