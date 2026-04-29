'use strict'
Object.defineProperty(exports, '__esModule', { value: true })

const path = require('path')

const include = path.join(__dirname, 'include/node')
const includeDir = path.relative(process.cwd(), include)
const jsLibrary = path.join(__dirname, './dist/library_napi.js').replace(/\\|\\\\/g, '/')
const sources = [
  path.join(__dirname, './src/js_native_api.c'),
  path.join(__dirname, './src/node_api.c'),
  path.join(__dirname, './src/async_cleanup_hook.c'),
  path.join(__dirname, './src/async_context.c'),
  path.join(__dirname, './src/async_work.c'),
  path.join(__dirname, './src/threadsafe_function.c'),
  path.join(__dirname, './src/uv/uv-common.c'),
  path.join(__dirname, './src/uv/threadpool.c'),
  path.join(__dirname, './src/uv/unix/loop.c'),
  path.join(__dirname, './src/uv/unix/posix-hrtime.c'),
  path.join(__dirname, './src/uv/unix/thread.c'),
  path.join(__dirname, './src/uv/unix/async.c'),
  path.join(__dirname, './src/uv/unix/core.c')
]
const targets = path.relative(process.cwd(), path.join(__dirname, 'emnapi.gyp'))

exports.include = include
exports.include_dir = includeDir
exports.js_library = jsLibrary
exports.sources = sources
exports.targets = targets

exports.requiredConfig = {
  emscripten: {
    settings: {
      WASM_BIGINT: '1',
      ALLOW_MEMORY_GROWTH: '1',
      MIN_CHROME_VERSION: '85',
      EXPORTED_RUNTIME_METHODS: [
        'emnapiInit'
      ],
      EXPORTED_FUNCTIONS: [
        '_malloc',
        '_free',
        '_napi_register_wasm_v1',
        '_node_api_module_get_api_version_v1'
      ]
    }
  },
  clang: {
    target: 'wasm32-wasip1-threads',
    cflags: ['-matomics', '-mbulk-memory'],
    ldflags: ['-mexec-model=reactor'],
    wasmld: [
      '--import-memory',
      '--shared-memory',
      '--export-table',
      '--export=malloc',
      '--export=free',
      '--export=napi_register_wasm_v1',
      '--export-if-defined=node_api_module_get_api_version_v1',
      '--export=emnapi_thread_crashed',
      '--export-if-defined=emnapi_async_worker_create',
      '--export-if-defined=emnapi_async_worker_init'
    ]
  }
}
