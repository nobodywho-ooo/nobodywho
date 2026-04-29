#include <stddef.h>
#include <stdint.h>

#if defined(__EMSCRIPTEN_PTHREADS__) || (defined(_REENTRANT) && defined(__wasi__))
#include <pthread.h>
#else
int _emnapi_spawn_worker(void* (*worker)(void*), void* arg);
void* calloc(size_t n, size_t size);

extern unsigned char __heap_base;
extern unsigned char __data_end;
extern unsigned char __global_base;
extern __attribute__((__weak__)) unsigned char __stack_high;
extern __attribute__((__weak__)) unsigned char __stack_low;

#define ROUND_UP(x, ALIGNMENT) (((x)+ALIGNMENT-1)&-ALIGNMENT)
#define STACK_ALIGN 16
#define DEFAULT_STACK_MAX (8<<20)

struct worker_args {
  void* stack_base;
  void* tls_base;
};

extern void __wasm_init_tls(void*);

__attribute__((__weak__))
void* __copy_tls(unsigned char *mem) {
	size_t tls_align = __builtin_wasm_tls_align();
	volatile void* tls_base = __builtin_wasm_tls_base();
	mem += tls_align;
	mem -= (uintptr_t)mem & (tls_align-1);
	__wasm_init_tls(mem);
  	__asm__("local.get %0\n"
			"global.set __tls_base\n"
			:: "r"(tls_base));
	return mem;
}
#endif

void* _emnapi_async_worker(void* arg);

__attribute__((visibility("default")))
void* emnapi_async_worker_create(int directly_spawn, void* global_address) {
#if defined(__EMSCRIPTEN_PTHREADS__) || (defined(_REENTRANT) && defined(__wasi__))
  if (directly_spawn) {
    pthread_t thread;
    int r = pthread_create(&thread, NULL, _emnapi_async_worker, global_address);
    if (r != 0) {
      return NULL;
    }
    return (void*)(uintptr_t)thread;
  }
  return NULL;
#else
  if (directly_spawn) {
    int index = _emnapi_spawn_worker(_emnapi_async_worker, global_address);
    return (void*)(intptr_t)(-(index + 1));
  }
  size_t args_size = sizeof(struct worker_args);
  size_t size = args_size;

  size_t tls_size = __builtin_wasm_tls_size();

  size_t tls_block_size = 0;
  if (tls_size) {
    size_t tls_align = __builtin_wasm_tls_align();
    tls_block_size = ROUND_UP(tls_size + tls_align - 1, tls_align);
    size += tls_block_size;
  }

  ptrdiff_t stack_size = 0;
  if (&__stack_high) {
		stack_size = &__stack_high - &__stack_low;
  } else {
		unsigned char *sp;
		__asm__(
			".globaltype __stack_pointer, i32\n"
			"global.get __stack_pointer\n"
			"local.set %0\n"
			: "=r"(sp));
		stack_size = sp > &__global_base ? &__heap_base - &__data_end : (ptrdiff_t)&__global_base;
	}

  stack_size = ROUND_UP(stack_size, STACK_ALIGN);
  stack_size = stack_size < DEFAULT_STACK_MAX ? stack_size : DEFAULT_STACK_MAX;
  size += stack_size;

  void* block = calloc(1, size);
  if (!block) return 0;

  void* tls_base = tls_size ? __copy_tls(block + args_size) : 0;

  struct worker_args* args = (struct worker_args*)block;

  args->stack_base = block + args_size + tls_block_size + stack_size;
  args->tls_base = tls_base;
  return block;
#endif
}
