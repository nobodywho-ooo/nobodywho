{
  'targets': [
    {
      'target_name': 'dlmalloc',
      'type': 'static_library',
      'defines': [
        'PAGESIZE=65536'
      ],
      'sources': [
        'src/malloc/sbrk.c',
        'src/malloc/memcpy.c',
        'src/malloc/memset.c',
        'src/malloc/dlmalloc/dlmalloc.c',
      ],
      'conditions': [
        ['wasm_threads != 0', {
          'defines': [
            'USE_LOCKS=1'
          ],
        }],
      ],
    },
    {
      'target_name': 'emmalloc',
      'type': 'static_library',
      'defines': [
        'PAGESIZE=65536'
      ],
      'sources': [
        'src/malloc/sbrk.c',
        'src/malloc/memcpy.c',
        'src/malloc/memset.c',
        'src/malloc/emmalloc/emmalloc.c',
      ],
      'cflags': [ '-fno-strict-aliasing' ],
      'xcode_settings': {
        'GCC_STRICT_ALIASING': 'NO',
      },
      'conditions': [
        ['wasm_threads != 0', {
          'defines': [
            '__EMSCRIPTEN_SHARED_MEMORY__=1'
          ],
        }],
      ],
    },
    {
      'target_name': 'emnapi_basic',
      'type': 'static_library',
      'defines': [
      ],
      'sources': [
        'src/js_native_api.c',
        'src/node_api.c',
        'src/async_cleanup_hook.c',
        'src/async_context.c',
        'src/wasi_wait.c',

        'src/uv/uv-common.c',
        'src/uv/threadpool.c',
        'src/uv/unix/loop.c',
        'src/uv/unix/posix-hrtime.c',
        'src/uv/unix/thread.c',
        'src/uv/unix/async.c',
        'src/uv/unix/core.c',
      ],
      'link_settings': {
        'target_conditions': [
          ['_type == "executable" and OS == "emscripten"', {
            'libraries': [
              '--js-library=<(emnapi_js_library)',
            ]
          }],
        ]
      },
      'conditions': [
        ['wasm_threads != 0 and OS in " unknown wasm wasi"', {
          'sources': [
            'src/thread/async_worker_create.c',
            'src/thread/async_worker_init.S',
          ],
          'link_settings': {
            'target_conditions': [
              ['_type == "executable"', {
                'ldflags': [
                  '-Wl,--export=emnapi_async_worker_create',
                  '-Wl,--export=emnapi_async_worker_init',
                ],
                'xcode_settings': {
                  'OTHER_LDFLAGS': [
                    '-Wl,--export=emnapi_async_worker_create',
                    '-Wl,--export=emnapi_async_worker_init',
                  ],
                },
              }],
            ]
          },
        }],
        ['OS == "wasi"', {
          'link_settings': {
            'target_conditions': [
              ['_type == "executable"', {
                'ldflags': [
                  '-Wl,--export=emnapi_thread_crashed',
                ],
                'xcode_settings': {
                  'OTHER_LDFLAGS': [
                    '-Wl,--export=emnapi_thread_crashed',
                  ],
                },
              }],
            ]
          },
        }],
      ]
    },
    {
      'target_name': 'emnapi',
      'type': 'static_library',
      'sources': [
        'src/js_native_api.c',
        'src/node_api.c',
        'src/async_cleanup_hook.c',
        'src/async_context.c',
        'src/wasi_wait.c',

        'src/uv/uv-common.c',
        'src/uv/threadpool.c',
        'src/uv/unix/loop.c',
        'src/uv/unix/posix-hrtime.c',
        'src/uv/unix/thread.c',
        'src/uv/unix/async.c',
        'src/uv/unix/core.c',

        'src/async_work.c',
        'src/threadsafe_function.c',
      ],
      'link_settings': {
        'target_conditions': [
          ['_type == "executable" and OS == "emscripten"', {
            'libraries': [
              '--js-library=<(emnapi_js_library)',
            ]
          }],
        ]
      },
    }
  ]
}
