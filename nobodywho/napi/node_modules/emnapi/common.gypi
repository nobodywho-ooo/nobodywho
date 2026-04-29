# This file is originally created by [RReverser](https://github.com/RReverser)
# in https://github.com/lovell/sharp/pull/3522
{
  'variables': {
    # OS: 'emscripten' | 'wasi' | 'unknown' | 'wasm'
    'clang': 1,
    'target_arch%': 'wasm32',
    'wasm_threads%': 0,
    'stack_size%': 1048576,
    'initial_memory%': 16777216,
    'max_memory%': 2147483648,
    # must be an absolute path
    'emnapi_js_library%': '<!(node -p "path.resolve(process.argv[1],\'dist/library_napi.js\').replace(process.argv[2],\'/\')" "<(node_root_dir)" "\\\\")',
    'emnapi_manual_linking%': 0,
  },

  'target_defaults': {
    'type': 'executable',

    'defines': [
      'BUILDING_NODE_EXTENSION',
      '__STDC_FORMAT_MACROS',
    ],

    'cflags': [
      '-Wall',
      '-Wextra',
      '-Wno-unused-parameter',
    ],
    'cflags_cc': [
      '-fno-rtti',
      '-fno-exceptions',
      '-std=c++17'
    ],

    'xcode_settings': {
      # WARNING_CFLAGS == cflags
      # OTHER_CFLAGS == cflags_c
      # OTHER_CPLUSPLUSFLAGS == cflags_cc
      # OTHER_LDFLAGS == ldflags

      'CLANG_CXX_LANGUAGE_STANDARD': 'c++17',
      'GCC_ENABLE_CPP_RTTI': 'NO',
      'GCC_ENABLE_CPP_EXCEPTIONS': 'NO',
      'WARNING_CFLAGS': [
        '-Wall',
        '-Wextra',
        '-Wno-unused-parameter',
      ]
    },

    'default_configuration': 'Release',
    'configurations': {
      'Debug': {
        'defines': [ 'DEBUG', '_DEBUG' ],
        'cflags': [ '-g', '-O0' ],
        'ldflags': [ '-g', '-O0' ],
        'xcode_settings': {
          'WARNING_CFLAGS': [ '-g', '-O0' ],
          'OTHER_LDFLAGS': [ '-g', '-O0' ],
        },
      },
      'Release': {
        'cflags': [ '-O3' ],
        'ldflags': [ '-O3' ],
        'xcode_settings': {
          'WARNING_CFLAGS': [ '-O3' ],
          'OTHER_LDFLAGS': [ '-O3' ],
        },
      }
    },

    'conditions': [
      ['OS == "emscripten"', {
        'defines': [
          'NAPI_EXTERN=__attribute__((__import_module__(\"env\")))'
        ],

        'cflags': [
          '-sDEFAULT_TO_CXX=0',
        ],
        'ldflags': [
          "-sALLOW_MEMORY_GROWTH=1",
          "-sEXPORTED_FUNCTIONS=['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']",
          '-sNODEJS_CATCH_EXIT=0',
          '-sNODEJS_CATCH_REJECTION=0',
          '-sWASM_BIGINT=1',
          '-sMIN_CHROME_VERSION=85',
          '-sMIN_NODE_VERSION=161500',
          '-sSTACK_SIZE=<(stack_size)',
          '-sDEFAULT_PTHREAD_STACK_SIZE=<(stack_size)',
          '-sINITIAL_MEMORY=<(initial_memory)',
          '-sMAXIMUM_MEMORY=<(max_memory)',
        ],
        'xcode_settings': {
          'WARNING_CFLAGS': [
            '-sDEFAULT_TO_CXX=0',
          ],
          'OTHER_LDFLAGS': [
            "-sALLOW_MEMORY_GROWTH=1",
            "-sEXPORTED_FUNCTIONS=['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']",
            '-sNODEJS_CATCH_EXIT=0',
            '-sNODEJS_CATCH_REJECTION=0',
            '-sWASM_BIGINT=1',
            '-sMIN_CHROME_VERSION=85',
            '-sMIN_NODE_VERSION=161500',
            '-sSTACK_SIZE=<(stack_size)',
            '-sDEFAULT_PTHREAD_STACK_SIZE=<(stack_size)',
            '-sINITIAL_MEMORY=<(initial_memory)',
            '-sMAXIMUM_MEMORY=<(max_memory)',
          ],
        },

        'configurations': {
          'Debug': {
            'ldflags': [ '-sSAFE_HEAP=1' ],
            'xcode_settings': {
              'OTHER_LDFLAGS': [ '-sSAFE_HEAP=1' ],
            },
          }
        },

        'conditions': [
          ['target_arch == "wasm64"', {
            'cflags': [ '-sMEMORY64=1' ],
            'ldflags': [ '-sMEMORY64=1' ],
            'xcode_settings': {
              'WARNING_CFLAGS': [ '-sMEMORY64=1' ],
              'OTHER_LDFLAGS': [ '-sMEMORY64=1' ],
            },
          }],
          ['wasm_threads != 0', {
            'cflags': [ '-sWASM_WORKERS=1', '-pthread' ],
            'ldflags': [ '-pthread' ],
            'xcode_settings': {
              'WARNING_CFLAGS': [ '-sWASM_WORKERS=1', '-pthread' ],
              'OTHER_LDFLAGS': [ '-pthread' ],
            },
          }],
        ],
      }], 
      ['OS in "wasi wasm unknown "', {
        'configurations': {
          'Release': {
            'ldflags': [ '-Wl,--strip-debug' ],
            'xcode_settings': {
              'OTHER_LDFLAGS': [ '-Wl,--strip-debug' ],
            },
          }
        },

        'conditions': [
          ['target_arch == "wasm64"', {
            'ldflags': [ "-Wl,-mwasm64" ],
            'xcode_settings': {
              'OTHER_LDFLAGS': [ "-Wl,-mwasm64" ],
            },
          }],
          ['wasm_threads != 0', {
            'cflags': [ "-matomics", "-mbulk-memory" ],
            'xcode_settings': {
              'WARNING_CFLAGS': [ "-matomics", "-mbulk-memory" ],
            },
          }],
          ['OS == "wasi"', {
            'ldflags': [
              '-mexec-model=reactor'
            ],
            'xcode_settings': {
              'OTHER_LDFLAGS': [
                '-mexec-model=reactor'
              ],
            },
            'conditions': [
              ['target_arch == "wasm64"', {
                'conditions': [
                  ['wasm_threads != 0', {
                    # wasi-threads
                    'cflags': [ '--target=wasm64-wasi-threads', '-pthread' ],
                    'ldflags': [ '--target=wasm64-wasi-threads', '-pthread' ],
                    'xcode_settings': {
                      'WARNING_CFLAGS': [ '--target=wasm64-wasi-threads', '-pthread' ],
                      'OTHER_LDFLAGS': [ '--target=wasm64-wasi-threads', '-pthread' ],
                    },
                  }, {
                    # wasi
                    'cflags': [ '--target=wasm64-wasi' ],
                    'ldflags': [ '--target=wasm64-wasi' ],
                    'xcode_settings': {
                      'WARNING_CFLAGS': [ '--target=wasm64-wasi' ],
                      'OTHER_LDFLAGS': [ '--target=wasm64-wasi' ],
                    },
                  }],
                ],
              }, {
                'conditions': [
                  ['wasm_threads != 0', {
                    # wasi-threads
                    'cflags': [ '--target=wasm32-wasi-threads', '-pthread' ],
                    'ldflags': [ '--target=wasm32-wasi-threads', '-pthread' ],
                    'xcode_settings': {
                      'WARNING_CFLAGS': [ '--target=wasm32-wasi-threads', '-pthread' ],
                      'OTHER_LDFLAGS': [ '--target=wasm32-wasi-threads', '-pthread' ],
                    },
                  }, {
                    # wasi
                    'cflags': [ '--target=wasm32-wasi' ],
                    'ldflags': [ '--target=wasm32-wasi' ],
                    'xcode_settings': {
                      'WARNING_CFLAGS': [ '--target=wasm32-wasi' ],
                      'OTHER_LDFLAGS': [ '--target=wasm32-wasi' ],
                    },
                  }],
                ],
              }],
            ],
          }, {
            'conditions': [
              ['target_arch == "wasm64"', {
                'cflags': [ '--target=wasm64-unknown-unknown' ],
                'ldflags': [ '--target=wasm64-unknown-unknown' ],
                'xcode_settings': {
                  'WARNING_CFLAGS': [ '--target=wasm64-unknown-unknown' ],
                  'OTHER_LDFLAGS': [ '--target=wasm64-unknown-unknown' ],
                },
              }, {
                'cflags': [ '--target=wasm32-unknown-unknown' ],
                'ldflags': [ '--target=wasm32-unknown-unknown' ],
                'xcode_settings': {
                  'WARNING_CFLAGS': [ '--target=wasm32-unknown-unknown' ],
                  'OTHER_LDFLAGS': [ '--target=wasm32-unknown-unknown' ],
                },
              }],
            ],
          }],
        ]
      }],
      ['emnapi_manual_linking != 0', {
        'target_conditions': [
          ['_type=="executable"', {
            'conditions': [
              ['OS in "wasi wasm unknown "', {
                'product_extension': 'wasm',

                'ldflags': [
                  '-Wl,--export-dynamic',
                  '-Wl,--export=malloc',
                  '-Wl,--export=free',
                  '-Wl,--export=napi_register_wasm_v1',
                  '-Wl,--export-if-defined=node_api_module_get_api_version_v1',
                  '-Wl,--import-undefined',
                  '-Wl,--export-table',
                  '-Wl,-zstack-size=<(stack_size)',
                  '-Wl,--initial-memory=<(initial_memory)',
                  '-Wl,--max-memory=<(max_memory)',
                ],
                'xcode_settings': {
                  'OTHER_LDFLAGS': [
                    '-Wl,--export-dynamic',
                    '-Wl,--export=malloc',
                    '-Wl,--export=free',
                    '-Wl,--export=napi_register_wasm_v1',
                    '-Wl,--export-if-defined=node_api_module_get_api_version_v1',
                    '-Wl,--import-undefined',
                    '-Wl,--export-table',
                    '-Wl,-zstack-size=<(stack_size)',
                    '-Wl,--initial-memory=<(initial_memory)',
                    '-Wl,--max-memory=<(max_memory)',
                  ],
                },

                'conditions': [
                  ['wasm_threads != 0', {
                    'ldflags': [
                      '-Wl,--import-memory',
                      '-Wl,--shared-memory',
                    ],
                    'xcode_settings': {
                      'OTHER_LDFLAGS': [
                        '-Wl,--import-memory',
                        '-Wl,--shared-memory',
                      ]
                    },
                  }],
                  ['OS != "wasi"', {
                    'ldflags': [
                      '-nostdlib',
                      '-Wl,--no-entry',
                    ],
                    'xcode_settings': {
                      'OTHER_LDFLAGS': [
                        '-nostdlib',
                        '-Wl,--no-entry',
                      ],
                    },
                  }],
                ]
              }]
            ],
          }],
        ],
      }, {
        'target_conditions': [
          ['_type=="executable"', {
            'sources': [
              'src/js_native_api.c',
              'src/node_api.c',
              'src/async_cleanup_hook.c',
              'src/async_context.c',
              'src/async_work.c',
              'src/wasi_wait.c',
              'src/threadsafe_function.c',
              'src/uv/uv-common.c',
              'src/uv/threadpool.c',
              'src/uv/unix/loop.c',
              'src/uv/unix/posix-hrtime.c',
              'src/uv/unix/thread.c',
              'src/uv/unix/async.c',
              'src/uv/unix/core.c',
            ],
            'conditions': [
              ['OS == "emscripten"', {
                'conditions': [
                  ['emnapi_js_library != ""', {
                    'libraries': [
                      '--js-library=<(emnapi_js_library)',
                    ],
                  }]
                ],
              }],
              ['OS in "wasi wasm unknown "', {
                'product_extension': 'wasm',

                'ldflags': [
                  '-Wl,--export-dynamic',
                  '-Wl,--export=malloc',
                  '-Wl,--export=free',
                  '-Wl,--export=napi_register_wasm_v1',
                  '-Wl,--export-if-defined=node_api_module_get_api_version_v1',
                  '-Wl,--import-undefined',
                  '-Wl,--export-table',
                  '-Wl,-zstack-size=<(stack_size)',
                  '-Wl,--initial-memory=<(initial_memory)',
                  '-Wl,--max-memory=<(max_memory)',
                ],
                'xcode_settings': {
                  'OTHER_LDFLAGS': [
                    '-Wl,--export-dynamic',
                    '-Wl,--export=malloc',
                    '-Wl,--export=free',
                    '-Wl,--export=napi_register_wasm_v1',
                    '-Wl,--export-if-defined=node_api_module_get_api_version_v1',
                    '-Wl,--import-undefined',
                    '-Wl,--export-table',
                    '-Wl,-zstack-size=<(stack_size)',
                    '-Wl,--initial-memory=<(initial_memory)',
                    '-Wl,--max-memory=<(max_memory)',
                  ],
                },

                'conditions': [
                  ['wasm_threads != 0', {
                    'ldflags': [
                      '-Wl,--import-memory',
                      '-Wl,--shared-memory',
                    ],
                    'xcode_settings': {
                      'OTHER_LDFLAGS': [
                        '-Wl,--import-memory',
                        '-Wl,--shared-memory',
                      ]
                    },
                  }],
                  ['OS == "wasi"', {
                    'ldflags': [
                      '-Wl,--export=emnapi_thread_crashed',
                    ],
                    'xcode_settings': {
                      'OTHER_LDFLAGS': [
                        '-Wl,--export=emnapi_thread_crashed',
                      ],
                    },
                  }],
                  ['OS != "wasi"', {
                    'defines': [
                      'PAGESIZE=65536'
                    ],
                    'ldflags': [
                      '-nostdlib',
                      '-Wl,--no-entry',
                    ],
                    'xcode_settings': {
                      'OTHER_LDFLAGS': [
                        '-nostdlib',
                        '-Wl,--no-entry',
                      ],
                    },
                    'sources': [
                      'src/malloc/sbrk.c',
                      'src/malloc/memcpy.c',
                      'src/malloc/memset.c',
                      'src/malloc/dlmalloc/dlmalloc.c',
                    ],
                    'conditions': [
                      ['wasm_threads != 0', {
                        # wasm32 + threads
                        'sources': [
                          'src/thread/async_worker_create.c',
                          'src/thread/async_worker_init.S',
                        ],
                        'defines': [
                          'USE_LOCKS=1'
                        ],
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
                  }],
                ]
              }]
            ],
          }],
        ],
      }]
    ],
  }
}
