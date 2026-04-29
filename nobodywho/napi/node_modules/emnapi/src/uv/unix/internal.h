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

#ifndef UV_UNIX_INTERNAL_H_
#define UV_UNIX_INTERNAL_H_

#include "../uv-common.h"
#include "emnapi_common.h"
#include <stdint.h>

#ifndef EMNAPI_NEXTTICK_TYPE
#define EMNAPI_NEXTTICK_TYPE 0
#endif
#if EMNAPI_NEXTTICK_TYPE == 0
EMNAPI_INTERNAL_EXTERN void _emnapi_set_immediate(void (*callback)(void*), void* data);
#define NEXT_TICK(callback, data) _emnapi_set_immediate((callback), (data))
#elif EMNAPI_NEXTTICK_TYPE == 1
EMNAPI_INTERNAL_EXTERN void _emnapi_next_tick(void (*callback)(void*), void* data);
#define NEXT_TICK(callback, data) _emnapi_next_tick((callback), (data))
#else
#error "Invalid EMNAPI_NEXTTICK_TYPE"
#endif

typedef enum {
  UV_CLOCK_PRECISE = 0,  /* Use the highest resolution clock available. */
  UV_CLOCK_FAST = 1      /* Use the fastest clock with <= 1ms granularity. */
} uv_clocktype_t;

uint64_t uv__hrtime(uv_clocktype_t type);

#if defined(__EMSCRIPTEN_PTHREADS__) || defined(_REENTRANT)
void uv__async_close(uv_async_t* handle);
void uv__make_close_pending(uv_handle_t* handle);
#endif

#endif /* UV_UNIX_INTERNAL_H_ */
