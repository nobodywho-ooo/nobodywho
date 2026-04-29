#if defined(__wasi__)

#include <time.h>
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include "emnapi_internal.h"

EMNAPI_INTERNAL_EXTERN void _emnapi_worker_ref(pthread_t pid);

struct __pthread {
  unsigned char _[32];
  volatile int cancel;
  volatile unsigned char canceldisable, cancelasync;
	unsigned char tsd_used:1;
	unsigned char dlerror_flag:1;
  unsigned char __[68];
};

#define INFINITY __builtin_inff()

int __wasilibc_futex_wait_atomic_wait(volatile void *addr, int op, int val, int64_t max_wait_ns);

static _Atomic pthread_t crashed_thread_id = NULL;

__attribute__((visibility("default")))
void emnapi_thread_crashed() {
  crashed_thread_id = pthread_self();
}

void _emnapi_yield() {
  if (crashed_thread_id) {
    _emnapi_worker_ref(crashed_thread_id);
    _emnapi_runtime_keepalive_push();
    _emnapi_unwind();
  }
}

static int _emnapi_wait_main_browser_thread(int is_runtime_thread, volatile void *addr, int op, int val, double max_wait_ms) {
  double now = _emnapi_get_now();
  double end = now + max_wait_ms;

  while (1) {
    now = _emnapi_get_now();
    if (now >= end) {
      return -ETIMEDOUT;
    }

    if (is_runtime_thread) {
      _emnapi_yield();
    }

    if (__c11_atomic_load((_Atomic int *)addr, __ATOMIC_SEQ_CST) != val) {
      break;
    }
  }

  return 0;
}

int _emnapi_wait(int is_runtime_thread, volatile void *addr, int op, int val, double max_wait_ms) {
  if (is_runtime_thread) {
    _emnapi_yield();
  }

  // https://github.com/WebAssembly/wasi-libc/blob/3f7eb4c7d6ede4dde3c4bffa6ed14e8d656fe93f/libc-top-half/musl/src/thread/wasm32/__wasilibc_busywait.c#L42
  if (!_emnapi_is_main_browser_thread()) {
    int64_t max_wait_ns = -1;
    if (max_wait_ms != INFINITY) {
      max_wait_ns = (int64_t)(max_wait_ms*1000*1000);
    }
    return __wasilibc_futex_wait_atomic_wait(addr, op, val, max_wait_ns);
  }

  return _emnapi_wait_main_browser_thread(is_runtime_thread, addr, op, val, max_wait_ms);
}

int __wasilibc_futex_wait_maybe_busy(volatile void *addr, int op, int val, int64_t max_wait_ns) {
  // https://github.com/emscripten-core/emscripten/blob/89ce854a99238d04116a3d9b5afe241eec90c6c3/system/lib/libc/musl/src/thread/__timedwait.c#L60
  int r = 0;
  double msecsToSleep = max_wait_ns >= 0 ? (max_wait_ns / 1000000.0) : INFINITY;
  int is_runtime_thread = _emnapi_is_main_runtime_thread();
	double max_ms_slice_to_sleep = is_runtime_thread ? 1 : 100;

  if (is_runtime_thread ||
	    pthread_self()->canceldisable != PTHREAD_CANCEL_DISABLE ||
	    pthread_self()->cancelasync) {
    double sleepUntilTime = _emnapi_get_now() + msecsToSleep;
		do {
      if (pthread_self()->cancel) {
				pthread_testcancel();
				return ECANCELED;
			}
			msecsToSleep = sleepUntilTime - _emnapi_get_now();
			if (msecsToSleep <= 0) {
				r = ETIMEDOUT;
				break;
			}
			if (msecsToSleep > max_ms_slice_to_sleep)
				msecsToSleep = max_ms_slice_to_sleep;
			r = -_emnapi_wait(is_runtime_thread, (void*)addr, op, val, msecsToSleep);
		} while (r == ETIMEDOUT);
  } else {
    r = -_emnapi_wait(is_runtime_thread, (void*)addr, op, val, msecsToSleep);
  }
  return r;
}

#endif
