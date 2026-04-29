#ifdef __wasm64__
#define PTR i64
#else
#define PTR i32
#endif

  .text

  .export_name  emnapi_async_worker_init, emnapi_async_worker_init

  .globaltype  __stack_pointer, PTR
  .globaltype  __tls_base, PTR

  .hidden  emnapi_async_worker_init
  .globl  emnapi_async_worker_init
  .type  emnapi_async_worker_init,@function

emnapi_async_worker_init:
  .functype  emnapi_async_worker_init (PTR) -> ()

  local.get   0
  PTR.load    0
  global.set  __stack_pointer

  local.get   0
#ifdef __wasm64__
  PTR.load    8
#else
  PTR.load    4
#endif
  global.set  __tls_base

  end_function
