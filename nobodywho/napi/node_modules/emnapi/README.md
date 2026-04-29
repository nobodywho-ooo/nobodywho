# emnapi

<p align="center">
  <img src="https://toyobayashi.github.io/emnapi-docs/emnapi.svg" alt="emnapi logo" width="256" />
</p>

## Sponsors

<p align="center">
  <a href="https://cdn.jsdelivr.net/gh/toyobayashi/toyobayashi/sponsorkit/sponsors.svg">
    <img src='https://cdn.jsdelivr.net/gh/toyobayashi/toyobayashi/sponsorkit/sponsors.svg'/>
  </a>
</p>

[![Build](https://github.com/toyobayashi/emnapi/actions/workflows/main.yml/badge.svg?branch=main)](https://github.com/toyobayashi/emnapi/actions/workflows/main.yml)

[Node-API](https://nodejs.org/docs/latest/api/n-api.html) implementation for [Emscripten](https://emscripten.org/index.html), [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) and clang with wasm support.

This project aims to

- Help users port their or existing Node-API native addons to wasm with code change as less as possible.
- Make runtime behavior matches native Node.js as much as possible.

This project also powers the WebAssembly feature for [napi-rs](https://github.com/napi-rs/napi-rs), and enables many Node.js native addons to run on [StackBlitz](https://stackblitz.com)'s WebContainer.

[Node-API changes](https://github.com/nodejs/node/pulls?q=is%3Apr+label%3Anode-api+) will be synchronized into this repo.

See documentation for more details:
- [https://toyobayashi.github.io/emnapi-docs/guide/](https://toyobayashi.github.io/emnapi-docs/guide/)
- [https://emnapi-docs.vercel.app/guide/](https://emnapi-docs.vercel.app/guide/)

中文文档：
- [https://toyobayashi.github.io/emnapi-docs/zh/guide/](https://toyobayashi.github.io/emnapi-docs/zh/guide/)
- [https://emnapi-docs.vercel.app/zh/guide/](https://emnapi-docs.vercel.app/zh/guide/)

[Full API List](https://toyobayashi.github.io/emnapi-docs/reference/list.html)

[How to build Node-API official examples](https://github.com/toyobayashi/node-addon-examples)

If you want to deep dive into WebAssembly, highly recommend you to visit [learn-wasm.dev](https://learn-wasm.dev?via=toyobayashi).

## Prerequests

You will need to install:

- Node.js `>= v16.15.0`
- npm `>= v8`
- Emscripten `>= v3.1.9` / wasi-sdk / LLVM clang with wasm support
- (Optional) CMake `>= v3.13`
- (Optional) node-gyp `>= v10.2.0`
- (Optional) ninja
- (Optional) make
- (Optional) [node-addon-api](https://github.com/nodejs/node-addon-api) `>= 6.1.0`

There are several choices to get `make` for Windows user

- Install [mingw-w64](https://www.mingw-w64.org/downloads/), then use `mingw32-make`
- Download [MSVC prebuilt binary of GNU make](https://github.com/toyobayashi/make-win-build/releases), add to `%Path%` then rename it to `mingw32-make`
- Install [Visual Studio 2022](https://visualstudio.microsoft.com/) C++ desktop workload, use `nmake` in `Visual Studio Developer Command Prompt`
- Install [Visual C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), use `nmake` in `Visual Studio Developer Command Prompt`

Verify your environment:

```bash
node -v
npm -v
emcc -v

# clang -v
# clang -print-targets # ensure wasm32 target exists

cmake --version

# if you use node-gyp
node-gyp --version

# if you use ninja
ninja --version

# if you use make
make -v

# if you use nmake in Visual Studio Developer Command Prompt
nmake /?
```

## Build from source

You need to set `EMSDK` and `WASI_SDK_PATH` environment variables.

```bash
git clone https://github.com/toyobayashi/emnapi.git
cd ./emnapi
npm install -g node-gyp
npm install
npm run build             # output ./packages/*/dist
node ./script/release.js  # output ./out

# test
npm run rebuild:test
npm test
```

See [CONTRIBUTING](https://github.com/toyobayashi/emnapi/blob/main/CONTRIBUTING.md) for more details.

## Quick Start

### NPM Install

```bash
npm install -D emnapi
npm install @emnapi/runtime

# for non-emscripten
npm install @emnapi/core

# if you use node-addon-api
npm install node-addon-api
```

Each package should match the same version.

### Using C

Create `hello.c`.

```c
#include <node_api.h>

#define NODE_API_CALL(env, the_call)                            \
  do {                                                          \
    if ((the_call) != napi_ok) {                                \
      const napi_extended_error_info *error_info;               \
      napi_get_last_error_info((env), &error_info);             \
      bool is_pending;                                          \
      const char* err_message = error_info->error_message;      \
      napi_is_exception_pending((env), &is_pending);            \
      if (!is_pending) {                                        \
        const char* error_message = err_message != NULL ?       \
          err_message :                                         \
          "empty error message";                                \
        napi_throw_error((env), NULL, error_message);           \
      }                                                         \
      return NULL;                                              \
    }                                                           \
  } while (0)

static napi_value js_hello(napi_env env, napi_callback_info info) {
  napi_value world;
  const char* str = "world";
  NODE_API_CALL(env, napi_create_string_utf8(env, str, NAPI_AUTO_LENGTH, &world));
  return world;
}

NAPI_MODULE_INIT() {
  napi_value hello;
  NODE_API_CALL(env, napi_create_function(env, "hello", NAPI_AUTO_LENGTH,
                                      js_hello, NULL, &hello));
  NODE_API_CALL(env, napi_set_named_property(env, exports, "hello", hello));
  return exports;
}
```

The C code is equivalant to the following JavaScript:

```js
module.exports = (function (exports) {
  const hello = function hello () {
    // native code in js_hello
    const world = 'world'
    return world
  }

  exports.hello = hello
  return exports
})(module.exports)
```

#### Building

<details>
<summary>emscripten</summary><br />

```bash
emcc -O3 \
     -DBUILDING_NODE_EXTENSION \
     "-DNAPI_EXTERN=__attribute__((__import_module__(\"env\")))" \
     -I./node_modules/emnapi/include/node \
     -L./node_modules/emnapi/lib/wasm32-emscripten \
     --js-library=./node_modules/emnapi/dist/library_napi.js \
     -sEXPORTED_FUNCTIONS="['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']" \
     -sEXPORTED_RUNTIME_METHODS=['emnapiInit'] \
     -o hello.js \
     hello.c \
     -lemnapi
```

</details>

<details>
<summary>wasi-sdk</summary><br />

```bash
clang -O3 \
      -DBUILDING_NODE_EXTENSION \
      -I./node_modules/emnapi/include/node \
      -L./node_modules/emnapi/lib/wasm32-wasi \
      --target=wasm32-wasi \
      --sysroot=$WASI_SDK_PATH/share/wasi-sysroot \
      -mexec-model=reactor \
      -Wl,--initial-memory=16777216 \
      -Wl,--export-dynamic \
      -Wl,--export=malloc \
      -Wl,--export=free \
      -Wl,--export=napi_register_wasm_v1 \
      -Wl,--export-if-defined=node_api_module_get_api_version_v1 \
      -Wl,--import-undefined \
      -Wl,--export-table \
      -o hello.wasm \
      hello.c \
      -lemnapi
```

</details>

<details>
<summary>clang wasm32</summary><br />

Choose `libdlmalloc.a` or `libemmalloc.a` for `malloc` and `free`.

```bash
clang -O3 \
      -DBUILDING_NODE_EXTENSION \
      -I./node_modules/emnapi/include/node \
      -L./node_modules/emnapi/lib/wasm32 \
      --target=wasm32 \
      -nostdlib \
      -Wl,--no-entry \
      -Wl,--initial-memory=16777216 \
      -Wl,--export-dynamic \
      -Wl,--export=malloc \
      -Wl,--export=free \
      -Wl,--export=napi_register_wasm_v1 \
      -Wl,--export-if-defined=node_api_module_get_api_version_v1 \
      -Wl,--import-undefined \
      -Wl,--export-table \
      -o hello.wasm \
      hello.c \
      -lemnapi \
      -ldlmalloc # -lemmalloc
```

</details>

#### Initialization

To initialize emnapi, you need to import the emnapi runtime to create a `Context` by `createContext` or `getDefaultContext` first.
Each context owns isolated Node-API object such as `napi_env`, `napi_value`, `napi_ref`. If you have multiple emnapi modules, you should reuse the same `Context` across them. 

```ts
declare namespace emnapi {
  // module '@emnapi/runtime'
  export class Context { /* ... */ }
  /** Create a new context */
  export function createContext (): Context
  /** Create or get */
  export function getDefaultContext (): Context
  // ...
}
```

<details>
<summary>emscripten</summary><br />

then call `Module.emnapiInit` after emscripten runtime initialized.
`Module.emnapiInit` only do initialization once, it will always return the same binding exports after successfully initialized.

```ts
declare namespace Module {
  interface EmnapiInitOptions {
    context: emnapi.Context

    /** node_api_get_module_file_name */
    filename?: string

    /**
     * Support following async_hooks related things
     * on Node.js runtime only
     * 
     * napi_async_init,
     * napi_async_destroy,
     * napi_make_callback,
     * async resource parameter of
     * napi_create_async_work and napi_create_threadsafe_function
     */
    nodeBinding?: typeof import('@emnapi/node-binding')

    /** See Multithread part */
    asyncWorkPoolSize?: number
  }
  export function emnapiInit (options: EmnapiInitOptions): any
}
```

```html
<script src="node_modules/@emnapi/runtime/dist/emnapi.min.js"></script>
<script src="hello.js"></script>
<script>
Module.onRuntimeInitialized = function () {
  var binding;
  try {
    binding = Module.emnapiInit({ context: emnapi.getDefaultContext() });
  } catch (err) {
    console.error(err);
    return;
  }
  var msg = 'hello ' + binding.hello();
  window.alert(msg);
};

// if -sMODULARIZE=1
Module({ /* Emscripten module init options */ }).then(function (Module) {
  var binding = Module.emnapiInit({ context: emnapi.getDefaultContext() });
});
</script>
```

If you are using `Visual Studio Code` and have `Live Server` extension installed, you can right click the HTML file in Visual Studio Code source tree and click `Open With Live Server`, then you can see the hello world alert!

Running on Node.js:

```js
const emnapi = require('@emnapi/runtime')
const Module = require('./hello.js')

Module.onRuntimeInitialized = function () {
  let binding
  try {
    binding = Module.emnapiInit({ context: emnapi.getDefaultContext() })
  } catch (err) {
    console.error(err)
    return
  }
  const msg = `hello ${binding.hello()}`
  console.log(msg)
}

// if -sMODULARIZE=1
Module({ /* Emscripten module init options */ }).then((Module) => {
  const binding = Module.emnapiInit({ context: emnapi.getDefaultContext() })
})
```

</details>

<details>
<summary>wasi-sdk or clang wasm32</summary><br />

For non-emscripten, you need to use `@emnapi/core`. The initialization is similar to emscripten.

```html
<script src="node_modules/@emnapi/runtime/dist/emnapi.min.js"></script>
<script src="node_modules/@emnapi/core/dist/emnapi-core.min.js"></script>
<script>
emnapiCore.instantiateNapiModule(fetch('./hello.wasm'), {
  context: emnapi.getDefaultContext(),
  overwriteImports (importObject) {
    // importObject.env = {
    //   ...importObject.env,
    //   ...importObject.napi,
    //   ...importObject.emnapi,
    //   // ...
    // }
  }
}).then(({ instance, module, napiModule }) => {
  const binding = napiModule.exports
  // ...
})
</script>
```

Using WASI on Node.js

```js
const { instantiateNapiModule } = require('@emnapi/core')
const { getDefaultContext } = require('@emnapi/runtime')
const { WASI } = require('wasi')
const fs = require('fs')

instantiateNapiModule(fs.promises.readFile('./hello.wasm'), {
  wasi: new WASI({ /* ... */ }),
  context: getDefaultContext(),
  overwriteImports (importObject) {
    // importObject.env = {
    //   ...importObject.env,
    //   ...importObject.napi,
    //   ...importObject.emnapi,
    //   // ...
    // }
  }
}).then(({ instance, module, napiModule }) => {
  const binding = napiModule.exports
  // ...
})
```

Using WASI on browser, you can use WASI polyfill in [wasm-util](https://github.com/toyobayashi/wasm-util),
and [memfs-browser](https://github.com/toyobayashi/memfs-browser)

```js
import { instantiateNapiModule } from '@emnapi/core'
import { getDefaultContext } from '@emnapi/runtime'
import { WASI } from '@tybys/wasm-util'
import { Volume, createFsFromVolume } from 'memfs-browser'

const fs = createFsFromVolume(Volume.fromJSON({ /* ... */ }))
instantiateNapiModule(fetch('./hello.wasm'), {
  wasi: new WASI({ fs, /* ... */ })
  context: getDefaultContext(),
  overwriteImports (importObject) {
    // importObject.env = {
    //   ...importObject.env,
    //   ...importObject.napi,
    //   ...importObject.emnapi,
    //   // ...
    // }
  }
}).then(({ instance, module, napiModule }) => {
  const binding = napiModule.exports
  // ...
})
```

</details>

### Using C++ and node-addon-api

Require [`node-addon-api`](https://github.com/nodejs/node-addon-api) `>= 6.1.0`

```bash
npm install node-addon-api
```

**Note: C++ wrapper can only be used to target Node.js v14.6.0+ and modern browsers those support `FinalizationRegistry` and `WeakRef` ([v8 engine v8.4+](https://v8.dev/blog/v8-release-84))!**

Create `hello.cpp`.

```cpp
#include <napi.h>

Napi::String Method(const Napi::CallbackInfo& info) {
  Napi::Env env = info.Env();
  return Napi::String::New(env, "world");
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
  exports.Set(Napi::String::New(env, "hello"),
              Napi::Function::New(env, Method)).Check();
  return exports;
}

NODE_API_MODULE(NODE_GYP_MODULE_NAME, Init)
```

Compile `hello.cpp` using `em++`. C++ exception is disabled by Emscripten default, and not supported by wasi-sdk, so predefine `-DNAPI_DISABLE_CPP_EXCEPTIONS` and `-DNODE_ADDON_API_ENABLE_MAYBE` here. If you would like to enable C++ exception, use `-sDISABLE_EXCEPTION_CATCHING=0` instead and remove `.Check()` call. See official documentation [here](https://github.com/nodejs/node-addon-api/blob/main/doc/error_handling.md).

#### Building

<details>
<summary>emscripten</summary><br />

```bash
em++ -O3 \
     -DBUILDING_NODE_EXTENSION \
     "-DNAPI_EXTERN=__attribute__((__import_module__(\"env\")))" \
     -DNAPI_DISABLE_CPP_EXCEPTIONS \
     -DNODE_ADDON_API_ENABLE_MAYBE \
     -I./node_modules/emnapi/include/node \
     -I./node_modules/node-addon-api \
     -L./node_modules/emnapi/lib/wasm32-emscripten \
     --js-library=./node_modules/emnapi/dist/library_napi.js \
     -sEXPORTED_FUNCTIONS="['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']" \
     -sEXPORTED_RUNTIME_METHODS=['emnapiInit'] \
     -o hello.js \
     hello.cpp \
     -lemnapi
```

</details>

<details>
<summary>wasi-sdk</summary><br />

```bash
clang++ -O3 \
        -DBUILDING_NODE_EXTENSION \
        -DNAPI_DISABLE_CPP_EXCEPTIONS \
        -DNODE_ADDON_API_ENABLE_MAYBE \
        -I./node_modules/emnapi/include/node \
        -I./node_modules/node-addon-api \
        -L./node_modules/emnapi/lib/wasm32-wasi \
        --target=wasm32-wasi \
        --sysroot=$WASI_SDK_PATH/share/wasi-sysroot \
        -fno-exceptions \
        -mexec-model=reactor \
        -Wl,--initial-memory=16777216 \
        -Wl,--export-dynamic \
        -Wl,--export=malloc \
        -Wl,--export=free \
        -Wl,--export=napi_register_wasm_v1 \
        -Wl,--export-if-defined=node_api_module_get_api_version_v1 \
        -Wl,--import-undefined \
        -Wl,--export-table \
        -o hello.wasm \
        hello.cpp \
        -lemnapi
```

</details>

<details>
<summary>clang wasm32</summary><br />

`node-addon-api` is using the C++ standard libraries, so you must use WASI if you are using `node-addon-api`.

You can still use `wasm32-unknown-unknown` target if you use Node-API C API only in C++.

```bash
clang++ -O3 \
        -DBUILDING_NODE_EXTENSION \
        -I./node_modules/emnapi/include/node \
        -L./node_modules/emnapi/lib/wasm32 \
        --target=wasm32 \
        -fno-exceptions \
        -nostdlib \
        -Wl,--no-entry \
        -Wl,--initial-memory=16777216 \
        -Wl,--export-dynamic \
        -Wl,--export=malloc \
        -Wl,--export=free \
        -Wl,--export=napi_register_wasm_v1 \
        -Wl,--export-if-defined=node_api_module_get_api_version_v1 \
        -Wl,--import-undefined \
        -Wl,--export-table \
        -o node_api_c_api_only.wasm \
        node_api_c_api_only.cpp \
        -lemnapi \
        -ldlmalloc # -lemmalloc
```

`operator new` and `operator delete`.

```cpp
#include <stddef.h>

extern "C" void* malloc(size_t size);
extern "C" void free(void* p);

void* operator new(size_t size) {
  return malloc(size);
}

void operator delete(void* p) noexcept {
  free(p);
}

void operator delete(void* p, size_t) noexcept {
  free(p);
}
```

</details>

### Using CMake

Create `CMakeLists.txt`.

```cmake
cmake_minimum_required(VERSION 3.13)

project(emnapiexample)

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/node_modules/emnapi")

add_executable(hello hello.c)

target_link_libraries(hello emnapi)
if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  target_link_options(hello PRIVATE
    "-sEXPORTED_FUNCTIONS=['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']"
    "-sEXPORTED_RUNTIME_METHODS=['emnapiInit']"
  )
elseif(CMAKE_SYSTEM_NAME STREQUAL "WASI")
  set_target_properties(hello PROPERTIES SUFFIX ".wasm")
  target_link_options(hello PRIVATE
    "-mexec-model=reactor"
    "-Wl,--export=napi_register_wasm_v1"
    "-Wl,--export-if-defined=node_api_module_get_api_version_v1"
    "-Wl,--initial-memory=16777216,--export-dynamic,--export=malloc,--export=free,--import-undefined,--export-table"
  )
elseif((CMAKE_C_COMPILER_TARGET STREQUAL "wasm32") OR (CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-unknown-unknown"))
  set_target_properties(hello PROPERTIES SUFFIX ".wasm")
  target_link_options(hello PRIVATE
    "-nostdlib"
    "-Wl,--export=napi_register_wasm_v1"
    "-Wl,--export-if-defined=node_api_module_get_api_version_v1"
    "-Wl,--no-entry"
    "-Wl,--initial-memory=16777216,--export-dynamic,--export=malloc,--export=free,--import-undefined,--export-table"
  )
  target_link_libraries(hello dlmalloc)
  # target_link_libraries(hello emmalloc)
endif()
```

If you use node-addon-api, you can use `-DEMNAPI_FIND_NODE_ADDON_API=ON` or manually add node-addon-api directory to the include dir via `include_directories()` or `target_include_directories()`.

```bash
mkdir build

# emscripten
emcmake cmake -DCMAKE_BUILD_TYPE=Release \
              -DEMNAPI_FIND_NODE_ADDON_API=ON \
              -G Ninja -H. -Bbuild

# wasi-sdk
cmake -DCMAKE_TOOLCHAIN_FILE=$WASI_SDK_PATH/share/cmake/wasi-sdk.cmake \
      -DWASI_SDK_PREFIX=$WASI_SDK_PATH \
      -DEMNAPI_FIND_NODE_ADDON_API=ON \
      -DCMAKE_BUILD_TYPE=Release \
      -G Ninja -H. -Bbuild

# wasm32
cmake -DCMAKE_TOOLCHAIN_FILE=node_modules/emnapi/cmake/wasm32.cmake \
      -DLLVM_PREFIX=$WASI_SDK_PATH \
      -DCMAKE_BUILD_TYPE=Release \
      -G Ninja -H. -Bbuild

cmake --build build
```

Output code can run in recent version modern browsers and Node.js latest LTS. IE is not supported.

### Using node-gyp (Experimental)

Require node-gyp `>= 10.2.0`

See [emnapi-node-gyp-test](https://github.com/toyobayashi/emnapi-node-gyp-test) for examples.

- Variables

Arch: `node-gyp configure --arch=<wasm32 | wasm64>`

```ts
// node-gyp configure -- -Dvariable_name=value

declare var OS: 'emscripten' | 'wasi' | 'unknown' | 'wasm' | ''

/**
 * Enable async work and threadsafe-functions
 * @default 0
 */
declare var wasm_threads: 0 | 1

/** @default 1048576 */
declare var stack_size: number

/** @default 16777216 */
declare var initial_memory: number

/** @default 2147483648 */
declare var max_memory: number

/** @default path.join(path.dirname(commonGypiPath,'./dist/library_napi.js')) */
declare var emnapi_js_library: string

/** @default 0 */
declare var emnapi_manual_linking: 0 | 1
```

- Create `binding.gyp`


```py
{
  "targets": [
    {
      "target_name": "hello",
      "sources": [
        "hello.c"
      ],
      "conditions": [
        ["OS == 'emscripten'", {
          "product_extension": "js", # required

          "cflags": [],
          "cflags_c": [],
          "cflags_cc": [],
          "ldflags": []
        }],
        ["OS == 'wasi'", {
          # ...
        }],
        ["OS in ' wasm unknown'", {
          # ...
        }]
      ]
    }
  ]
}
```

- Add the following environment variables.

```bash
# Linux or macOS
export GYP_CROSSCOMPILE=1

# emscripten
export AR_target="$EMSDK/upstream/emscripten/emar"
export CC_target="$EMSDK/upstream/emscripten/emcc"
export CXX_target="$EMSDK/upstream/emscripten/em++"

# wasi-sdk
export AR_target="$WASI_SDK_PATH/bin/ar"
export CC_target="$WASI_SDK_PATH/bin/clang"
export CXX_target="$WASI_SDK_PATH/bin/clang++"
```

```bat
@REM Windows

set GYP_CROSSCOMPILE=1

@REM emscripten
call set AR_target=%%EMSDK:\=/%%/upstream/emscripten/emar.bat
call set CC_target=%%EMSDK:\=/%%/upstream/emscripten/emcc.bat
call set CXX_target=%%EMSDK:\=/%%/upstream/emscripten/em++.bat

@REM wasi-sdk
call set AR_target=%%WASI_SDK_PATH:\=/%%/bin/ar.exe
call set CC_target=%%WASI_SDK_PATH:\=/%%/bin/clang.exe
call set CXX_target=%%WASI_SDK_PATH:\=/%%/bin/clang++.exe
```

- Build

```bash
# Linux or macOS

# emscripten
emmake node-gyp rebuild \
  --arch=wasm32 \
  --nodedir=./node_modules/emnapi \
  -- -f make-emscripten # -Dwasm_threads=1

# wasi
node-gyp rebuild \
  --arch=wasm32 \
  --nodedir=./node_modules/emnapi \
  -- -f make-wasi # -Dwasm_threads=1

# bare wasm32
node-gyp rebuild \
  --arch=wasm32 \
  --nodedir=./node_modules/emnapi \
  -- -f make-wasm # -Dwasm_threads=1
```

```bat
@REM Use make generator on Windows
@REM Run the bat file in POSIX-like environment (e.g. Cygwin)

@REM emscripten
call npx.cmd node-gyp configure --arch=wasm32 --nodedir=./node_modules/emnapi -- -f make-emscripten
call emmake.bat make -C %~dp0build

@REM wasi
call npx.cmd node-gyp configure --arch=wasm32 --nodedir=./node_modules/emnapi -- -f make-wasi
make -C %~dp0build

@REM bare wasm32
call npx.cmd node-gyp configure --arch=wasm32 --nodedir=./node_modules/emnapi -- -f make-wasm
make -C %~dp0build
```

### Using Rust

See [napi-rs](https://github.com/napi-rs/napi-rs) 

### Multithread

Related API:

- [napi_*_async_work](https://nodejs.org/dist/latest/docs/api/n-api.html#napi_create_async_work)
- [napi_*_threadsafe_function](https://nodejs.org/dist/latest/docs/api/n-api.html#asynchronous-thread-safe-function-calls)

They are available in emnapi, but you need to know more details before you start to use them.
Now emnapi has 3 implementations of async work and 2 implementations of TSFN:

- Async work
    - A. Libuv threadpool and pthread based implementation in C
    - B. Single thread mock in JavaScript
    - C. Web worker based implementation in C (stack allocation) and JavaScript
- TSFN
    - D. Libuv and pthread based implementation in C
    - E. Web worker based implementation in JavaScript

|   | Library to Link        | `wasm32-emscripten` | `wasm32` | `wasm32-wasi` | `wasm32-wasi-threads` |
|---|------------------------|---------------------|----------|---------------|-----------------------|
| A | libemnapi-mt.a         | ✅                   | ❌        | ❌             | ✅                     |
| B | libemnapi-basic(-mt).a | ✅                   | ✅        | ✅             | ✅                     |
| C | libemnapi-basic-mt.a   | ❌                   | ✅        | ❌             | ✅                     |
| D | libemnapi-mt.a         | ✅                   | ❌        | ❌             | ✅                     |
| E | libemnapi-basic(-mt).a | ✅                   | ✅        | ✅             | ✅                     |

There are some limitations on browser about wasi-libc's pthread implementation, for example
`pthread_mutex_lock` may call `__builtin_wasm_memory_atomic_wait32`(`memory.atomic.wait32`)
which is disallowed in browser JS main thread. While Emscripten's pthread implementation
has considered usage in browser. This issue can be solved by upgrading `wasi-sdk` to v26+
and emnapi v1.5.0+ then pass `--export=emnapi_thread_crashed` to the linker. If you need to
run your addon with multithreaded features, we recommend you use A & D or C & E.

Note: For browsers, all the multithreaded features relying on Web Workers (Emscripten pthread also relying on Web Workers)
require cross-origin isolation to enable `SharedArrayBuffer`. You can make a page cross-origin isolated
by serving the page with these headers:

```
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Opener-Policy: same-origin
```

If you would like to avoid `SharedArrayBuffer` and cross-origin isolation, please use B & E (link against `libemnapi-basic.a`), see the following table for more details.

#### About Prebuilt Libraries

Prebuilt libraries can be found in the `lib` directory in `emnapi` npm package.

| Library              | Description                                                                                                                                                                                                                                                   | `wasm32-emscripten` | `wasm32` | `wasm32-wasi` | `wasm32-wasi-threads`                   |
|----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------------------|----------|---------------|-----------------------------------------|
| libemnapi.a          | no atomics feature.<br/><br/> no libuv port.<br/><br/> `napi_*_async_work` and `napi_*_threadsafe_function` always return `napi_generic_failure`.                                                                                                                         | ✅                   | ✅        | ✅             | ✅ |
| libemnapi-mt.a       | atomics feature enabled.<br/><br/> `napi_*_async_work` and `napi_*_threadsafe_function` are based on pthread and libuv port.                                                                                                                                        | ✅                   | ❌        | ❌             | ✅ |
| libemnapi-basic.a    | no atomics feature.<br/><br/> no libuv port.<br/><br/> `napi_*_async_work` and `napi_*_threadsafe_function` are imported from JavaScript land.                                                                                                                            | ✅                   | ✅        | ✅             | ✅ |
| libemnapi-basic-mt.a | atomics feature enabled.<br/><br/> no libuv port.<br/><br/> `napi_*_async_work` and `napi_*_threadsafe_function` are imported from JavaScript land.<br/><br/> include `emnapi_async_worker_create` and `emnapi_async_worker_init` for WebWorker based async work implementation. | ❌                   | ✅        | ✅             | ✅ |
| libdlmalloc.a        | no atomics feature, no thread safe garanteed.                                                                                                                                                                                                                 | ❌                   | ✅        | ❌             | ❌                                       |
| libdlmalloc-mt.a     | atomics feature enabled, thread safe.                                                                                                                                                                                                                         | ❌                   | ✅        | ❌             | ❌                                       |
| libemmalloc.a        | no atomics feature, no thread safe garanteed.                                                                                                                                                                                                                 | ❌                   | ✅        | ❌             | ❌                                       |
| libemmalloc-mt.a     | atomics feature enabled, thread safe.                                                                                                                                                                                                                         | ❌                   | ✅        | ❌             | ❌                                       |

#### Usage

```cmake
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/node_modules/emnapi")

add_executable(hello hello.c)

if(CMAKE_SYSTEM_NAME STREQUAL "Emscripten")
  target_link_libraries(hello emnapi-mt)
  target_compile_options(hello PRIVATE "-pthread")
  target_link_options(hello PRIVATE
    "-sALLOW_MEMORY_GROWTH=1"
    "-sEXPORTED_FUNCTIONS=['_malloc','_free','_napi_register_wasm_v1','_node_api_module_get_api_version_v1']"
    "-sEXPORTED_RUNTIME_METHODS=['emnapiInit']"
    "-pthread"
    "-sPTHREAD_POOL_SIZE=4"
    # try to specify stack size if you experience pthread errors
    "-sSTACK_SIZE=2MB"
    "-sDEFAULT_PTHREAD_STACK_SIZE=2MB"
  )
elseif(CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-wasi-threads")
  target_link_libraries(hello emnapi-mt)
  set_target_properties(hello PROPERTIES SUFFIX ".wasm")
  target_compile_options(hello PRIVATE "-fno-exceptions" "-pthread")
  target_link_options(hello PRIVATE
    "-pthread"
    "-mexec-model=reactor"
    "-Wl,--import-memory"
    "-Wl,--max-memory=2147483648"
    "-Wl,--export-dynamic"
    "-Wl,--export=napi_register_wasm_v1"
    "-Wl,--export-if-defined=node_api_module_get_api_version_v1"
    "-Wl,--export=malloc"
    "-Wl,--export=free"
    "-Wl,--export=emnapi_thread_crashed"
    "-Wl,--import-undefined"
    "-Wl,--export-table"
  )
elseif((CMAKE_C_COMPILER_TARGET STREQUAL "wasm32") OR (CMAKE_C_COMPILER_TARGET STREQUAL "wasm32-unknown-unknown"))
  target_link_libraries(hello emnapi-basic-mt)
  set_target_properties(hello PROPERTIES SUFFIX ".wasm")
  target_compile_options(hello PRIVATE "-fno-exceptions" "-matomics" "-mbulk-memory")
  target_link_options(hello PRIVATE
    "-nostdlib"
    "-Wl,--no-entry"
    "-Wl,--export=napi_register_wasm_v1"
    "-Wl,--export-if-defined=node_api_module_get_api_version_v1"
    "-Wl,--export=emnapi_async_worker_create"
    "-Wl,--export=emnapi_async_worker_init"
    "-Wl,--import-memory,--shared-memory,--max-memory=2147483648,--import-undefined"
    "-Wl,--export-dynamic,--export=malloc,--export=free,--export-table"
  )
endif()
```

```bash
# emscripten
emcmake cmake -DCMAKE_BUILD_TYPE=Release \
              -DEMNAPI_FIND_NODE_ADDON_API=ON \
              -DEMNAPI_WORKER_POOL_SIZE=4 \
              -G Ninja -H. -Bbuild

# wasi-sdk with thread support
cmake -DCMAKE_TOOLCHAIN_FILE=$WASI_SDK_PATH/share/cmake/wasi-sdk-pthread.cmake \
      -DWASI_SDK_PREFIX=$WASI_SDK_PATH \
      -DEMNAPI_FIND_NODE_ADDON_API=ON \
      -DCMAKE_BUILD_TYPE=Release \
      -G Ninja -H. -Bbuild

cmake -DCMAKE_TOOLCHAIN_FILE=node_modules/emnapi/cmake/wasm32.cmake \
      -DLLVM_PREFIX=$WASI_SDK_PATH \
      -DCMAKE_BUILD_TYPE=Release \
      -G Ninja -H. -Bbuild

cmake --build build
```

And additional work is required during instantiating wasm compiled with non-emscripten.

```js
// emnapi main thread (could be in a Worker)
instantiateNapiModule(input, {
  context: getDefaultContext(),
  /**
   * emscripten
   *   0: no effect
   *   > 0: the same effect to UV_THREADPOOL_SIZE
   * non-emscripten
   *   0: single thread mock
   *   > 0 schedule async work in web worker
   */
  asyncWorkPoolSize: 4, // 0: single thread mock, > 0: schedule async work in web worker
  wasi: new WASI(/* ... */),

  /**
   * Setting this to `true` or a delay (ms) makes
   * pthread_create() do not return until worker actually start.
   * It will throw error if emnapi runs in browser main thread
   * since browser disallow blocking the main thread (Atomics.wait).
   * @defaultValue false
   */
  waitThreadStart: isNode || (isBrowser && !isBrowserMainThread),

  /**
   * Reuse the thread worker after thread exit to avoid re-creatation
   * @defaultValue false
   */
  reuseWorker: {
    /**
     * @see {@link https://emscripten.org/docs/tools_reference/settings_reference.html#pthread-pool-size | PTHREAD_POOL_SIZE}
     */
    size: 0,

    /**
     * @see {@link https://emscripten.org/docs/tools_reference/settings_reference.html#pthread-pool-size-strict | PTHREAD_POOL_SIZE_STRICT}
     */
    strict: false
  },

  onCreateWorker () {
    return new Worker('./worker.js')
    // Node.js
    // const { Worker } = require('worker_threads')
    // return new Worker(join(__dirname, './worker.js'), {
    //   env: process.env,
    //   execArgv: ['--experimental-wasi-unstable-preview1']
    // })
  },
  overwriteImports (importObject) {
    importObject.env.memory = new WebAssembly.Memory({
      initial: 16777216 / 65536,
      maximum: 2147483648 / 65536,
      shared: true
    })
  }
})
```

```js
// worker.js
(function () {
  let fs, WASI, emnapiCore

  const ENVIRONMENT_IS_NODE =
    typeof process === 'object' && process !== null &&
    typeof process.versions === 'object' && process.versions !== null &&
    typeof process.versions.node === 'string'

  if (ENVIRONMENT_IS_NODE) {
    const nodeWorkerThreads = require('worker_threads')

    const parentPort = nodeWorkerThreads.parentPort

    parentPort.on('message', (data) => {
      globalThis.onmessage({ data })
    })

    fs = require('fs')

    Object.assign(globalThis, {
      self: globalThis,
      require,
      Worker: nodeWorkerThreads.Worker,
      importScripts: function (f) {
        (0, eval)(fs.readFileSync(f, 'utf8') + '//# sourceURL=' + f)
      },
      postMessage: function (msg) {
        parentPort.postMessage(msg)
      }
    })

    WASI = require('wasi').WASI
    emnapiCore = require('@emnapi/core')
  } else {
    importScripts('./node_modules/memfs-browser/dist/memfs.js')
    importScripts('./node_modules/@tybys/wasm-util/dist/wasm-util.min.js')
    importScripts('./node_modules/@emnapi/core/dist/emnapi-core.js')
    emnapiCore = globalThis.emnapiCore

    const { Volume, createFsFromVolume } = memfs
    fs = createFsFromVolume(Volume.fromJSON({
      '/': null
    }))

    WASI = globalThis.wasmUtil.WASI
  }

  const { instantiateNapiModuleSync, MessageHandler } = emnapiCore

  const handler = new MessageHandler({
    onLoad ({ wasmModule, wasmMemory }) {
      const wasi = new WASI({ fs })

      return instantiateNapiModuleSync(wasmModule, {
        childThread: true,
        wasi,
        overwriteImports (importObject) {
          importObject.env.memory = wasmMemory
        }
      })
    }
  })

  globalThis.onmessage = function (e) {
    handler.handle(e)
    // handle other messages
  }
})()
```

## Preprocess Macro Options

### `-DEMNAPI_WORKER_POOL_SIZE=4`

This is [`UV_THREADPOOL_SIZE`](http://docs.libuv.org/en/v1.x/threadpool.html?highlight=UV_THREADPOOL_SIZE) equivalent at compile time, if not predefined, emnapi will read `asyncWorkPoolSize` option or `UV_THREADPOOL_SIZE` from Emscripten [environment variable](https://emscripten.org/docs/porting/connecting_cpp_and_javascript/Interacting-with-code.html#interacting-with-code-environment-variables) at runtime:

```js
Module.init({
  // ...
  asyncWorkPoolSize: 2
})

// if asyncWorkPoolSize is not specified
Module.preRun = Module.preRun || [];
Module.preRun.push(function () {
  if (typeof ENV !== 'undefined') {
    ENV.UV_THREADPOOL_SIZE = '2';
  }
});
```

```js
// wasi
instantiateNapiModule({
  // ...
  asyncWorkPoolSize: 2
})
// if asyncWorkPoolSize is not specified
new WASI({
  env: {
    UV_THREADPOOL_SIZE: '2'
  }
})
```

It represent max of `EMNAPI_WORKER_POOL_SIZE` async work (`napi_queue_async_work`) can be executed in parallel. Default is not defined.

You can set both `PTHREAD_POOL_SIZE` and `EMNAPI_WORKER_POOL_SIZE` to `number of CPU cores` in general.
If you use another library function which may create `N` child threads in async work,
then you need to set `PTHREAD_POOL_SIZE` to `EMNAPI_WORKER_POOL_SIZE * (N + 1)`.

This option only has effect if you use `-pthread`.
Emnapi will create `EMNAPI_WORKER_POOL_SIZE` threads when initializing,
it will throw error if `PTHREAD_POOL_SIZE < EMNAPI_WORKER_POOL_SIZE && PTHREAD_POOL_SIZE_STRICT == 2`.

See [Issue #8](https://github.com/toyobayashi/emnapi/issues/8) for more detail.

### `-DEMNAPI_NEXTTICK_TYPE=0`

This option only has effect if you use `-pthread`, Default is `0`.
Tell emnapi how to delay async work in `uv_async_send` / `uv__async_close`.

- `0`: Use `setImmediate()` (Node.js native `setImmediate` or browser `MessageChannel` and `port.postMessage`)
- `1`: Use `Promise.resolve().then()`

### `-DEMNAPI_USE_PROXYING=1`

This option only has effect if you use emscripten `-pthread`. Default is `1` if emscripten version `>= 3.1.9`, else `0`.

- `0`

    Use JavaScript implementation to send async work from worker threads, runtime code will access the Emscripten internal `PThread` object to add custom worker message listener.

- `1`:

    Use Emscripten [proxying API](https://emscripten.org/docs/api_reference/proxying.h.html) to send async work from worker threads in C. If you experience something wrong, you can switch set this to `0` and feel free to create an issue.
