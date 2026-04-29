#include <stddef.h>

#define SIZE_MAX -1

void *sbrk(ptrdiff_t increment) {
  // sbrk(0) returns the current memory size.
  if (increment == 0) {
    // The wasm spec doesn't guarantee that memory.grow of 0 always succeeds.
    return (void *)(__builtin_wasm_memory_size(0) * PAGESIZE);
  }

  // We only support page-size increments.
  if (increment % PAGESIZE != 0) {
    __builtin_trap();
  }

  // WebAssembly doesn't support shrinking linear memory.
  if (increment < 0) {
    __builtin_trap();
  }

  ptrdiff_t old = __builtin_wasm_memory_grow(0, (ptrdiff_t)increment / PAGESIZE);

  if (old == SIZE_MAX) {
    return (void *)-1;
  }

  return (void *)(old * PAGESIZE);
}
