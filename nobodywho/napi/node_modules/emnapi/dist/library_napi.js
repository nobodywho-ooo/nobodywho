{{{ ((typeof DEFAULT_LIBRARY_FUNCS_TO_INCLUDE.add === "function" ? DEFAULT_LIBRARY_FUNCS_TO_INCLUDE.add("$emnapiInit") : (DEFAULT_LIBRARY_FUNCS_TO_INCLUDE.indexOf("$emnapiInit") === -1 ? DEFAULT_LIBRARY_FUNCS_TO_INCLUDE.push("$emnapiInit") : undefined)), "") }}}
{{{ ((typeof EXPORTED_RUNTIME_METHODS.add === "function" ? EXPORTED_RUNTIME_METHODS.add("emnapiInit") : (EXPORTED_RUNTIME_METHODS.indexOf("emnapiInit") === -1 ? EXPORTED_RUNTIME_METHODS.push("emnapiInit") : undefined)), "") }}}
/* eslint-disable no-unreachable */
/* eslint-disable no-new-func */
/* eslint-disable @typescript-eslint/no-implied-eval */
var emnapiCtx = undefined;
var emnapiNodeBinding = undefined;
var emnapiAsyncWorkPoolSize = 0;
var emnapiModule = {
    exports: {},
    loaded: false,
    filename: ''
};
/**
 * @__deps napi_register_wasm_v1
 * @__deps node_api_module_get_api_version_v1
 */
function emnapiInit(options) {
    if (emnapiModule.loaded)
        return emnapiModule.exports;
    if (typeof options !== 'object' || options === null) {
        throw new TypeError('Invalid emnapi init option');
    }
    var context = options.context;
    if (typeof context !== 'object' || context === null) {
        throw new TypeError("Invalid `options.context`. Use `import { getDefaultContext } from '@emnapi/runtime'`");
    }
    emnapiCtx = context;
    var filename = typeof options.filename === 'string' ? options.filename : '';
    emnapiModule.filename = filename;
    if ('nodeBinding' in options) {
        var nodeBinding = options.nodeBinding;
        if (typeof nodeBinding !== 'object' || nodeBinding === null) {
            throw new TypeError('Invalid `options.nodeBinding`. Use @emnapi/node-binding package');
        }
        emnapiNodeBinding = nodeBinding;
    }
    if ('asyncWorkPoolSize' in options) {
        if (typeof options.asyncWorkPoolSize !== 'number') {
            throw new TypeError('options.asyncWorkPoolSize must be a integer');
        }
        emnapiAsyncWorkPoolSize = options.asyncWorkPoolSize >> 0;
        if (emnapiAsyncWorkPoolSize > 1024) {
            emnapiAsyncWorkPoolSize = 1024;
        }
        else if (emnapiAsyncWorkPoolSize < -1024) {
            emnapiAsyncWorkPoolSize = -1024;
        }
    }
    var moduleApiVersion = _node_api_module_get_api_version_v1();
    // eslint-disable-next-line @typescript-eslint/prefer-nullish-coalescing
    var envObject = emnapiModule.envObject || (emnapiModule.envObject = emnapiCtx.createEnv(filename, moduleApiVersion, function (cb) { return {{{ makeDynCall('vppp', 'cb') }}}; }, function (cb) { return {{{ makeDynCall('vp', 'cb') }}}; }, abort, emnapiNodeBinding));
    var scope = emnapiCtx.openScope(envObject);
    try {
        envObject.callIntoModule(function (_envObject) {
            var exports$1 = emnapiModule.exports;
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var exportsHandle = scope.add(exports$1);
            var napiValue = _napi_register_wasm_v1({{{ to64('_envObject.id') }}}, {{{ to64('exportsHandle.id') }}});
            emnapiModule.exports = (!napiValue) ? exports$1 : emnapiCtx.handleStore.get(napiValue).value;
        });
    }
    catch (err) {
        if (err !== 'unwind') {
            throw err;
        }
    }
    finally {
        emnapiCtx.closeScope(envObject, scope);
    }
    emnapiModule.loaded = true;
    delete emnapiModule.envObject;
    return emnapiModule.exports;
}
/**
 * @__sig i
 */
function __emnapi_async_work_pool_size() {
    return Math.abs(emnapiAsyncWorkPoolSize);
}
/**
 * @__sig ipiip
 */
function _napi_set_last_error(env, error_code, engine_error_code, engine_reserved) {
    var envObject = emnapiCtx.envStore.get(env);
    return envObject.setLastError(error_code, engine_error_code, engine_reserved);
}
/**
 * @__sig ip
 */
function _napi_clear_last_error(env) {
    var envObject = emnapiCtx.envStore.get(env);
    return envObject.clearLastError();
}
/**
 * @__sig vppp
 */
function __emnapi_get_node_version(major, minor, patch) {
    {{{ from64('major') }}};
    {{{ from64('minor') }}};
    {{{ from64('patch') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var versions = (typeof process === 'object' && process !== null &&
        typeof process.versions === 'object' && process.versions !== null &&
        typeof process.versions.node === 'string')
        ? process.versions.node.split('.').map(function (n) { return Number(n); })
        : [0, 0, 0];
    {{{ makeSetValue('major', 0, 'versions[0]', 'u32') }}};
    {{{ makeSetValue('minor', 0, 'versions[1]', 'u32') }}};
    {{{ makeSetValue('patch', 0, 'versions[2]', 'u32') }}};
}
/**
 * @__sig v
 * @__deps $runtimeKeepalivePush
 */
function __emnapi_runtime_keepalive_push() {
    if (typeof runtimeKeepalivePush === 'function')
        runtimeKeepalivePush();
}
/**
 * @__sig v
 * @__deps $runtimeKeepalivePop
 */
function __emnapi_runtime_keepalive_pop() {
    if (typeof runtimeKeepalivePop === 'function')
        runtimeKeepalivePop();
}
/**
 * @__sig vpp
 */
function __emnapi_set_immediate(callback, data) {
    emnapiCtx.feature.setImmediate(function () {
        {{{ makeDynCall('vp', 'callback') }}}(data);
    });
}
/**
 * @__sig vpp
 */
function __emnapi_next_tick(callback, data) {
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    Promise.resolve().then(function () {
        {{{ makeDynCall('vp', 'callback') }}}(data);
    });
}
/**
 * @__sig vipppi
 */
function __emnapi_callback_into_module(forceUncaught, env, callback, data, close_scope_if_throw) {
    var envObject = emnapiCtx.envStore.get(env);
    var scope = emnapiCtx.openScope(envObject);
    try {
        envObject.callbackIntoModule(Boolean(forceUncaught), function () {
            {{{ makeDynCall('vpp', 'callback') }}}(env, data);
        });
    }
    catch (err) {
        emnapiCtx.closeScope(envObject, scope);
        if (close_scope_if_throw) {
            emnapiCtx.closeScope(envObject);
        }
        throw err;
    }
    emnapiCtx.closeScope(envObject, scope);
}
/**
 * @__sig vipppp
 */
function __emnapi_call_finalizer(forceUncaught, env, callback, data, hint) {
    var envObject = emnapiCtx.envStore.get(env);
    {{{ from64('callback') }}};
    envObject.callFinalizerInternal(forceUncaught, callback, data, hint);
}
/**
 * @__sig v
 */
function __emnapi_ctx_increase_waiting_request_counter() {
    emnapiCtx.increaseWaitingRequestCounter();
}
/**
 * @__sig v
 */
function __emnapi_ctx_decrease_waiting_request_counter() {
    emnapiCtx.decreaseWaitingRequestCounter();
}
/**
 * @__sig i
 */
function __emnapi_is_main_runtime_thread() {
    return ENVIRONMENT_IS_PTHREAD ? 0 : 1;
}
/**
 * @__sig i
 */
function __emnapi_is_main_browser_thread() {
    return (typeof window !== 'undefined' && typeof document !== 'undefined' && !ENVIRONMENT_IS_NODE) ? 1 : 0;
}
/**
 * @__sig v
 */
function __emnapi_unwind() {
    // eslint-disable-next-line @typescript-eslint/no-throw-literal
    throw 'unwind';
}
/**
 * @__sig d
 */
function __emnapi_get_now() {
    return performance.timeOrigin + performance.now();
}
function emnapiSetValueI64(result, numberValue) {
    var tempDouble;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var tempI64 = [
        numberValue >>> 0,
        (tempDouble = numberValue, +Math.abs(tempDouble) >= 1 ? tempDouble > 0 ? (Math.min(+Math.floor(tempDouble / 4294967296), 4294967295) | 0) >>> 0 : ~~+Math.ceil((tempDouble - +(~~tempDouble >>> 0)) / 4294967296) >>> 0 : 0)
    ];
    {{{ makeSetValue('result', 0, 'tempI64[0]', 'i32') }}};
    {{{ makeSetValue('result', 4, 'tempI64[1]', 'i32') }}};
}
/**
 * @__deps $emnapiCtx
 * @__sig p
 */
function __emnapi_open_handle_scope() {
    return emnapiCtx.openScope().id;
}
/**
 * @__deps $emnapiCtx
 * @__sig vp
 */
function __emnapi_close_handle_scope(_scope) {
    return emnapiCtx.closeScope();
}
/* eslint-disable eqeqeq */
/* eslint-disable @typescript-eslint/no-unused-vars */
/* eslint-disable @typescript-eslint/indent */
/* eslint-disable @typescript-eslint/no-unnecessary-type-assertion */
/* eslint-disable @typescript-eslint/explicit-function-return-type */
/**
 * @__deps emscripten_resize_heap
 * @__sig ipjp
 */
function _napi_adjust_external_memory(env, low, high, adjusted_value) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    var change_in_bytes;
#if WASM_BIGINT
    if (!high)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    change_in_bytes = Number(low);
#else
    if (!adjusted_value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    change_in_bytes = (low >>> 0) + (high * Math.pow(2, 32));
#endif
    if (change_in_bytes < 0) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    if (change_in_bytes > 0) {
        var old_size = wasmMemory.buffer.byteLength;
        var new_size = old_size + change_in_bytes;
        if (!_emscripten_resize_heap(new_size)) {
            return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
        }
    }
#if WASM_BIGINT
    {{{ from64('high') }}};
    if (emnapiCtx.feature.supportBigInt) {
        {{{ makeSetValue('high', 0, 'wasmMemory.buffer.byteLength', 'i64') }}};
    }
    else {
        emnapiSetValueI64(high, wasmMemory.buffer.byteLength);
    }
#else
    {{{ from64('adjusted_value') }}};
    {{{ makeSetValue('adjusted_value', 0, 'wasmMemory.buffer.byteLength', 'i64') }}};
#endif
    return envObject.clearLastError();
}
/**
 * @__deps $PThread
 * @__sig vp
 */
function __emnapi_worker_ref(pid) {
    var worker = PThread.pthreads[pid];
    worker = worker.worker || worker;
    if (typeof worker.ref === 'function') {
        worker.ref();
    }
}
/**
 * @__deps $PThread
 * @__sig vp
 */
function __emnapi_worker_unref(pid) {
    var worker = PThread.pthreads[pid];
    worker = worker.worker || worker;
    if (typeof worker.unref === 'function') {
        worker.unref();
    }
}
/**
 * if EMNAPI_USE_PROXYING=1 (default is 1 if emscripten version >= 3.1.9),
 * the following helpers won't be linked into runtime code
 *
 * @__deps $PThread
 * @__postset
 * ```
 * PThread.unusedWorkers.forEach(emnapiAddSendListener);
 * PThread.runningWorkers.forEach(emnapiAddSendListener);
 * (function () {
 *   var __original_getNewWorker = PThread.getNewWorker;
 *   PThread.getNewWorker = function () {
 *     var r = __original_getNewWorker.apply(this, arguments);
 *     emnapiAddSendListener(r);
 *     return r;
 *   };
 * })();
 * ```
 */
function emnapiAddSendListener(worker) {
    if (!worker)
        return false;
    if (worker._emnapiSendListener)
        return true;
    var handler = function (e) {
        var data = ENVIRONMENT_IS_NODE ? e : e.data;
        var __emnapi__ = data.__emnapi__;
        if (__emnapi__ && __emnapi__.type === 'async-send') {
            if (ENVIRONMENT_IS_PTHREAD) {
                postMessage({ __emnapi__: __emnapi__ });
            }
            else {
                // eslint-disable-next-line @typescript-eslint/no-unused-vars
                var callback = __emnapi__.payload.callback;
                {{{ makeDynCall('vp', 'callback') }}}(__emnapi__.payload.data);
            }
        }
    };
    var dispose = function () {
        if (ENVIRONMENT_IS_NODE) {
            worker.off('message', handler);
        }
        else {
            worker.removeEventListener('message', handler, false);
        }
        delete worker._emnapiSendListener;
    };
    worker._emnapiSendListener = { handler: handler, dispose: dispose };
    if (ENVIRONMENT_IS_NODE) {
        worker.on('message', handler);
    }
    else {
        worker.addEventListener('message', handler, false);
    }
    return true;
}
/**
 * @__sig vipp
 */
function __emnapi_async_send_js(type, callback, data) {
    if (ENVIRONMENT_IS_PTHREAD) {
        postMessage({
            __emnapi__: {
                type: 'async-send',
                payload: {
                    callback: callback,
                    data: data
                }
            }
        });
    }
    else {
        switch (type) {
            case 0:
                __emnapi_set_immediate(callback, data);
                break;
            case 1:
                __emnapi_next_tick(callback, data);
                break;
            default: break;
        }
    }
}
/**
 * @__postset
 * ```
 * emnapiAWST.init();
 * ```
 */
var emnapiAWST = {
    idGen: {},
    values: [undefined],
    queued: new Set(),
    pending: [],
    init: function () {
        var idGen = {
            nextId: 1,
            list: [],
            generate: function () {
                var id;
                if (idGen.list.length) {
                    id = idGen.list.shift();
                }
                else {
                    id = idGen.nextId;
                    idGen.nextId++;
                }
                return id;
            },
            reuse: function (id) {
                idGen.list.push(id);
            }
        };
        emnapiAWST.idGen = idGen;
        emnapiAWST.values = [undefined];
        emnapiAWST.queued = new Set();
        emnapiAWST.pending = [];
    },
    create: function (env, resource, resourceName, execute, complete, data) {
        var asyncId = 0;
        var triggerAsyncId = 0;
        if (emnapiNodeBinding) {
            var asyncContext = emnapiNodeBinding.node.emitAsyncInit(resource, resourceName, -1);
            asyncId = asyncContext.asyncId;
            triggerAsyncId = asyncContext.triggerAsyncId;
        }
        var id = emnapiAWST.idGen.generate();
        emnapiAWST.values[id] = {
            env: env,
            id: id,
            resource: resource,
            asyncId: asyncId,
            triggerAsyncId: triggerAsyncId,
            status: 0,
            execute: execute,
            complete: complete,
            data: data
        };
        return id;
    },
    callComplete: function (work, status) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var complete = work.complete;
        var env = work.env;
        var data = work.data;
        var callback = function () {
            if (!complete)
                return;
            var envObject = emnapiCtx.envStore.get(env);
            var scope = emnapiCtx.openScope(envObject);
            try {
                envObject.callbackIntoModule(true, function () {
                    {{{ makeDynCall('vpip', 'complete') }}}(env, status, data);
                });
            }
            finally {
                emnapiCtx.closeScope(envObject, scope);
            }
        };
        if (emnapiNodeBinding) {
            emnapiNodeBinding.node.makeCallback(work.resource, callback, [], {
                asyncId: work.asyncId,
                triggerAsyncId: work.triggerAsyncId
            });
        }
        else {
            callback();
        }
    },
    queue: function (id) {
        var work = emnapiAWST.values[id];
        if (!work)
            return;
        if (work.status === 0) {
            work.status = 1;
            if (emnapiAWST.queued.size >= (Math.abs(emnapiAsyncWorkPoolSize) || 4)) {
                emnapiAWST.pending.push(id);
                return;
            }
            emnapiAWST.queued.add(id);
            var env_1 = work.env;
            var data_1 = work.data;
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var execute = work.execute;
            work.status = 2;
            emnapiCtx.feature.setImmediate(function () {
                {{{ makeDynCall('vpp', 'execute') }}}(env_1, data_1);
                emnapiAWST.queued.delete(id);
                work.status = 3;
                emnapiCtx.feature.setImmediate(function () {
                    emnapiAWST.callComplete(work, 0 /* napi_status.napi_ok */);
                });
                if (emnapiAWST.pending.length > 0) {
                    var nextWorkId = emnapiAWST.pending.shift();
                    emnapiAWST.values[nextWorkId].status = 0;
                    emnapiAWST.queue(nextWorkId);
                }
            });
        }
    },
    cancel: function (id) {
        var index = emnapiAWST.pending.indexOf(id);
        if (index !== -1) {
            var work_1 = emnapiAWST.values[id];
            if (work_1 && (work_1.status === 1)) {
                work_1.status = 4;
                emnapiAWST.pending.splice(index, 1);
                emnapiCtx.feature.setImmediate(function () {
                    emnapiAWST.callComplete(work_1, 11 /* napi_status.napi_cancelled */);
                });
                return 0 /* napi_status.napi_ok */;
            }
            else {
                return 9 /* napi_status.napi_generic_failure */;
            }
        }
        return 9 /* napi_status.napi_generic_failure */;
    },
    remove: function (id) {
        var work = emnapiAWST.values[id];
        if (!work)
            return;
        if (emnapiNodeBinding) {
            emnapiNodeBinding.node.emitAsyncDestroy({
                asyncId: work.asyncId,
                triggerAsyncId: work.triggerAsyncId
            });
        }
        emnapiAWST.values[id] = undefined;
        emnapiAWST.idGen.reuse(id);
    }
};
/**
 * @__sig ippppppp
 */
function _napi_create_async_work(env, resource, resource_name, execute, complete, data, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!execute)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var resourceObject;
    if (resource) {
        resourceObject = Object(emnapiCtx.handleStore.get(resource).value);
    }
    else {
        resourceObject = {};
    }
    if (!resource_name)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var resourceName = String(emnapiCtx.handleStore.get(resource_name).value);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var id = emnapiAWST.create(env, resourceObject, resourceName, execute, complete, data);
    {{{ makeSetValue('result', 0, 'id', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipp
 */
function _napi_delete_async_work(env, work) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!work)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    emnapiAWST.remove(work);
    return envObject.clearLastError();
}
/**
 * @__sig ipp
 */
function _napi_queue_async_work(env, work) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!work)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    emnapiAWST.queue(work);
    return envObject.clearLastError();
}
/**
 * @__sig ipp
 */
function _napi_cancel_async_work(env, work) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!work)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var status = emnapiAWST.cancel(work);
    if (status === 0 /* napi_status.napi_ok */)
        return envObject.clearLastError();
    return envObject.setLastError(status);
}
/* eslint-disable @typescript-eslint/indent */
/**
 * @__postset
 * ```
 * emnapiString.init();
 * ```
 */
var emnapiString = {
    utf8Decoder: undefined,
    utf16Decoder: undefined,
    init: function () {
#if !TEXTDECODER || TEXTDECODER == 1
        var fallbackDecoder = {
            decode: function (bytes) {
                var inputIndex = 0;
                var pendingSize = Math.min(0x1000, bytes.length + 1);
                var pending = new Uint16Array(pendingSize);
                var chunks = [];
                var pendingIndex = 0;
                for (;;) {
                    var more = inputIndex < bytes.length;
                    if (!more || (pendingIndex >= pendingSize - 1)) {
                        var subarray = pending.subarray(0, pendingIndex);
                        var arraylike = subarray;
                        chunks.push(String.fromCharCode.apply(null, arraylike));
                        if (!more) {
                            return chunks.join('');
                        }
                        bytes = bytes.subarray(inputIndex);
                        inputIndex = 0;
                        pendingIndex = 0;
                    }
                    var byte1 = bytes[inputIndex++];
                    if ((byte1 & 0x80) === 0) {
                        pending[pendingIndex++] = byte1;
                    }
                    else if ((byte1 & 0xe0) === 0xc0) {
                        var byte2 = bytes[inputIndex++] & 0x3f;
                        pending[pendingIndex++] = ((byte1 & 0x1f) << 6) | byte2;
                    }
                    else if ((byte1 & 0xf0) === 0xe0) {
                        var byte2 = bytes[inputIndex++] & 0x3f;
                        var byte3 = bytes[inputIndex++] & 0x3f;
                        pending[pendingIndex++] = ((byte1 & 0x1f) << 12) | (byte2 << 6) | byte3;
                    }
                    else if ((byte1 & 0xf8) === 0xf0) {
                        var byte2 = bytes[inputIndex++] & 0x3f;
                        var byte3 = bytes[inputIndex++] & 0x3f;
                        var byte4 = bytes[inputIndex++] & 0x3f;
                        var codepoint = ((byte1 & 0x07) << 0x12) | (byte2 << 0x0c) | (byte3 << 0x06) | byte4;
                        if (codepoint > 0xffff) {
                            codepoint -= 0x10000;
                            pending[pendingIndex++] = (codepoint >>> 10) & 0x3ff | 0xd800;
                            codepoint = 0xdc00 | codepoint & 0x3ff;
                        }
                        pending[pendingIndex++] = codepoint;
                    }
                    else {
                        // invalid
                    }
                }
            }
        };
#endif
        var utf8Decoder;
#if !TEXTDECODER
        utf8Decoder = fallbackDecoder;
#elif TEXTDECODER == 1
        utf8Decoder = typeof TextDecoder === 'function' ? new TextDecoder() : fallbackDecoder;
#elif TEXTDECODER == 2
        utf8Decoder = new TextDecoder();
#endif
        emnapiString.utf8Decoder = utf8Decoder;
#if !TEXTDECODER || TEXTDECODER == 1
        var fallbackDecoder2 = {
            decode: function (input) {
                var bytes = new Uint16Array(input.buffer, input.byteOffset, input.byteLength / 2);
                if (bytes.length <= 0x1000) {
                    return String.fromCharCode.apply(null, bytes);
                }
                var chunks = [];
                var i = 0;
                var len = 0;
                for (; i < bytes.length; i += len) {
                    len = Math.min(0x1000, bytes.length - i);
                    chunks.push(String.fromCharCode.apply(null, bytes.subarray(i, i + len)));
                }
                return chunks.join('');
            }
        };
#endif
        var utf16Decoder;
#if !TEXTDECODER
        utf16Decoder = fallbackDecoder2;
#elif TEXTDECODER == 1
        utf16Decoder = typeof TextDecoder === 'function' ? new TextDecoder('utf-16le') : fallbackDecoder2;
#elif TEXTDECODER == 2
        utf16Decoder = new TextDecoder('utf-16le');
#endif
        emnapiString.utf16Decoder = utf16Decoder;
    },
    lengthBytesUTF8: function (str) {
        var c;
        var len = 0;
        for (var i = 0; i < str.length; ++i) {
            c = str.charCodeAt(i);
            if (c <= 0x7F) {
                len++;
            }
            else if (c <= 0x7FF) {
                len += 2;
            }
            else if (c >= 0xD800 && c <= 0xDFFF) {
                len += 4;
                ++i;
            }
            else {
                len += 3;
            }
        }
        return len;
    },
    UTF8ToString: function (ptr, length) {
        if (!ptr || !length)
            return '';
        ptr >>>= 0;
        var HEAPU8 = new Uint8Array(wasmMemory.buffer);
        var end = ptr;
        if (length === -1) {
            for (; HEAPU8[end];)
                ++end;
        }
        else {
            end = ptr + (length >>> 0);
        }
#if TEXTDECODER != 2
        length = end - ptr;
        if (length <= 16) {
            var idx = ptr;
            var str = '';
            while (idx < end) {
                var u0 = HEAPU8[idx++];
                if (!(u0 & 0x80)) {
                    str += String.fromCharCode(u0);
                    continue;
                }
                var u1 = HEAPU8[idx++] & 63;
                if ((u0 & 0xE0) === 0xC0) {
                    str += String.fromCharCode(((u0 & 31) << 6) | u1);
                    continue;
                }
                var u2 = HEAPU8[idx++] & 63;
                if ((u0 & 0xF0) === 0xE0) {
                    u0 = ((u0 & 15) << 12) | (u1 << 6) | u2;
                }
                else {
                    u0 = ((u0 & 7) << 18) | (u1 << 12) | (u2 << 6) | (HEAPU8[idx++] & 63);
                }
                if (u0 < 0x10000) {
                    str += String.fromCharCode(u0);
                }
                else {
                    var ch = u0 - 0x10000;
                    str += String.fromCharCode(0xD800 | (ch >> 10), 0xDC00 | (ch & 0x3FF));
                }
            }
            return str;
        }
#endif
        return emnapiString.utf8Decoder.decode({{{ getUnsharedTextDecoderView('HEAPU8', 'ptr', 'end') }}});
    },
    stringToUTF8: function (str, outPtr, maxBytesToWrite) {
        var HEAPU8 = new Uint8Array(wasmMemory.buffer);
        var outIdx = outPtr;
        outIdx >>>= 0;
        if (!(maxBytesToWrite > 0)) {
            return 0;
        }
        var startIdx = outIdx;
        var endIdx = outIdx + maxBytesToWrite - 1;
        for (var i = 0; i < str.length; ++i) {
            var u = str.charCodeAt(i);
            if (u >= 0xD800 && u <= 0xDFFF) {
                var u1 = str.charCodeAt(++i);
                u = 0x10000 + ((u & 0x3FF) << 10) | (u1 & 0x3FF);
            }
            if (u <= 0x7F) {
                if (outIdx >= endIdx)
                    break;
                HEAPU8[outIdx++] = u;
            }
            else if (u <= 0x7FF) {
                if (outIdx + 1 >= endIdx)
                    break;
                HEAPU8[outIdx++] = 0xC0 | (u >> 6);
                HEAPU8[outIdx++] = 0x80 | (u & 63);
            }
            else if (u <= 0xFFFF) {
                if (outIdx + 2 >= endIdx)
                    break;
                HEAPU8[outIdx++] = 0xE0 | (u >> 12);
                HEAPU8[outIdx++] = 0x80 | ((u >> 6) & 63);
                HEAPU8[outIdx++] = 0x80 | (u & 63);
            }
            else {
                if (outIdx + 3 >= endIdx)
                    break;
                HEAPU8[outIdx++] = 0xF0 | (u >> 18);
                HEAPU8[outIdx++] = 0x80 | ((u >> 12) & 63);
                HEAPU8[outIdx++] = 0x80 | ((u >> 6) & 63);
                HEAPU8[outIdx++] = 0x80 | (u & 63);
            }
        }
        HEAPU8[outIdx] = 0;
        return outIdx - startIdx;
    },
    UTF16ToString: function (ptr, length) {
        if (!ptr || !length)
            return '';
        ptr >>>= 0;
        var end = ptr;
        if (length === -1) {
            var idx = end >> 1;
            var HEAPU16 = new Uint16Array(wasmMemory.buffer);
            while (HEAPU16[idx])
                ++idx;
            end = idx << 1;
        }
        else {
            end = ptr + (length >>> 0) * 2;
        }
#if TEXTDECODER != 2
        length = end - ptr;
        if (length <= 32) {
            return String.fromCharCode.apply(null, new Uint16Array(wasmMemory.buffer, ptr, length / 2));
        }
#endif
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var HEAPU8 = new Uint8Array(wasmMemory.buffer);
        return emnapiString.utf16Decoder.decode({{{ getUnsharedTextDecoderView('HEAPU8', 'ptr', 'end') }}});
    },
    stringToUTF16: function (str, outPtr, maxBytesToWrite) {
        if (maxBytesToWrite === undefined) {
            maxBytesToWrite = 0x7FFFFFFF;
        }
        if (maxBytesToWrite < 2)
            return 0;
        maxBytesToWrite -= 2;
        var startPtr = outPtr;
        var numCharsToWrite = (maxBytesToWrite < str.length * 2) ? (maxBytesToWrite / 2) : str.length;
        for (var i = 0; i < numCharsToWrite; ++i) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var codeUnit = str.charCodeAt(i);
            {{{ makeSetValue('outPtr', 0, 'codeUnit', 'i16') }}};
            outPtr += 2;
        }
        {{{ makeSetValue('outPtr', 0, '0', 'i16') }}};
        return outPtr - startPtr;
    },
    newString: function (env, str, length, result, stringMaker) {
        {{{ from64('length') }}};
        if (!env)
            return 1 /* napi_status.napi_invalid_arg */;
        // @ts-expect-error
        var envObject = emnapiCtx.envStore.get(env);
        envObject.checkGCAccess();
        var autoLength = length === -1;
        var sizelength = length >>> 0;
        if (length !== 0) {
            if (!str)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!(autoLength || (sizelength <= 2147483647)))
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('str') }}};
        var strValue = stringMaker(str, autoLength, sizelength);
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var value = emnapiCtx.addToCurrentScope(strValue).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.clearLastError();
    },
    newExternalString: function (env, str, length, finalize_callback, finalize_hint, result, copied, createApi, stringMaker) {
        {{{ from64('length') }}};
        if (!env)
            return 1 /* napi_status.napi_invalid_arg */;
        // @ts-expect-error
        var envObject = emnapiCtx.envStore.get(env);
        envObject.checkGCAccess();
        var autoLength = length === -1;
        var sizelength = length >>> 0;
        if (length !== 0) {
            if (!str)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!(autoLength || (sizelength <= 2147483647)))
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var status = createApi(env, str, length, result);
        if (status === 0 /* napi_status.napi_ok */) {
            if (copied) {
                {{{ makeSetValue('copied', 0, '1', 'i8') }}};
            }
            if (finalize_callback) {
                envObject.callFinalizer(finalize_callback, str, finalize_hint);
            }
        }
        return status;
    }
};
// eslint-disable-next-line @typescript-eslint/no-unused-vars
/**
 * @__sig ipp
 */
function _emnapi_get_module_object(env, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(Module);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippp
 */
function _emnapi_get_module_property(env, utf8name, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!utf8name)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('utf8name') }}};
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(Module[emnapiString.UTF8ToString(utf8name, -1)]);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__deps malloc
 * @__deps free
 * @__postset
 * ```
 * emnapiExternalMemory.init();
 * ```
 */
var emnapiExternalMemory = {
    registry: typeof FinalizationRegistry === 'function' ? new FinalizationRegistry(function (_pointer) { _free({{{ to64('_pointer') }}}); }) : undefined,
    table: new WeakMap(),
    wasmMemoryViewTable: new WeakMap(),
    init: function () {
        emnapiExternalMemory.registry = typeof FinalizationRegistry === 'function' ? new FinalizationRegistry(function (_pointer) { _free({{{ to64('_pointer') }}}); }) : undefined;
        emnapiExternalMemory.table = new WeakMap();
        emnapiExternalMemory.wasmMemoryViewTable = new WeakMap();
    },
    isSharedArrayBuffer: function (value) {
        return ((typeof SharedArrayBuffer === 'function' && value instanceof SharedArrayBuffer) ||
            (Object.prototype.toString.call(value) === '[object SharedArrayBuffer]'));
    },
    isDetachedArrayBuffer: function (arrayBuffer) {
        if (arrayBuffer.byteLength === 0) {
            try {
                // eslint-disable-next-line no-new
                new Uint8Array(arrayBuffer);
            }
            catch (_) {
                return true;
            }
        }
        return false;
    },
    getArrayBufferPointer: function (arrayBuffer, shouldCopy) {
        var _a;
        var info = {
            address: 0,
            ownership: 0 /* ReferenceOwnership.kRuntime */,
            runtimeAllocated: 0
        };
        if (arrayBuffer === wasmMemory.buffer) {
            return info;
        }
        var isDetached = emnapiExternalMemory.isDetachedArrayBuffer(arrayBuffer);
        if (emnapiExternalMemory.table.has(arrayBuffer)) {
            var cachedInfo = emnapiExternalMemory.table.get(arrayBuffer);
            if (isDetached) {
                cachedInfo.address = 0;
                return cachedInfo;
            }
            if (shouldCopy && cachedInfo.ownership === 0 /* ReferenceOwnership.kRuntime */ && cachedInfo.runtimeAllocated === 1) {
                new Uint8Array(wasmMemory.buffer).set(new Uint8Array(arrayBuffer), cachedInfo.address);
            }
            return cachedInfo;
        }
        if (isDetached || (arrayBuffer.byteLength === 0)) {
            return info;
        }
        if (!shouldCopy) {
            return info;
        }
        var pointer = _malloc({{{ to64('arrayBuffer.byteLength') }}});
        if (!pointer)
            throw new Error('Out of memory');
        new Uint8Array(wasmMemory.buffer).set(new Uint8Array(arrayBuffer), pointer);
        info.address = pointer;
        info.ownership = emnapiExternalMemory.registry ? 0 /* ReferenceOwnership.kRuntime */ : 1 /* ReferenceOwnership.kUserland */;
        info.runtimeAllocated = 1;
        emnapiExternalMemory.table.set(arrayBuffer, info);
        (_a = emnapiExternalMemory.registry) === null || _a === void 0 ? void 0 : _a.register(arrayBuffer, pointer);
        return info;
    },
    getOrUpdateMemoryView: function (view) {
        if (view.buffer === wasmMemory.buffer) {
            if (!emnapiExternalMemory.wasmMemoryViewTable.has(view)) {
                emnapiExternalMemory.wasmMemoryViewTable.set(view, {
                    Ctor: view.constructor,
                    address: view.byteOffset,
                    length: view instanceof DataView ? view.byteLength : view.length,
                    ownership: 1 /* ReferenceOwnership.kUserland */,
                    runtimeAllocated: 0
                });
            }
            return view;
        }
        var maybeOldWasmMemory = emnapiExternalMemory.isDetachedArrayBuffer(view.buffer) ||
            ((typeof SharedArrayBuffer === 'function') && (view.buffer instanceof SharedArrayBuffer));
        if (maybeOldWasmMemory && emnapiExternalMemory.wasmMemoryViewTable.has(view)) {
            var info = emnapiExternalMemory.wasmMemoryViewTable.get(view);
            var Ctor = info.Ctor;
            var newView = void 0;
            var Buffer = emnapiCtx.feature.Buffer;
            if (typeof Buffer === 'function' && Ctor === Buffer) {
                newView = Buffer.from(wasmMemory.buffer, info.address, info.length);
            }
            else {
                newView = new Ctor(wasmMemory.buffer, info.address, info.length);
            }
            emnapiExternalMemory.wasmMemoryViewTable.set(newView, info);
            return newView;
        }
        return view;
    },
    getViewPointer: function (view, shouldCopy) {
        view = emnapiExternalMemory.getOrUpdateMemoryView(view);
        if (view.buffer === wasmMemory.buffer) {
            if (emnapiExternalMemory.wasmMemoryViewTable.has(view)) {
                var _a = emnapiExternalMemory.wasmMemoryViewTable.get(view), address_1 = _a.address, ownership_1 = _a.ownership, runtimeAllocated_1 = _a.runtimeAllocated;
                return { address: address_1, ownership: ownership_1, runtimeAllocated: runtimeAllocated_1, view: view };
            }
            return { address: view.byteOffset, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0, view: view };
        }
        var _b = emnapiExternalMemory.getArrayBufferPointer(view.buffer, shouldCopy), address = _b.address, ownership = _b.ownership, runtimeAllocated = _b.runtimeAllocated;
        return { address: address === 0 ? 0 : (address + view.byteOffset), ownership: ownership, runtimeAllocated: runtimeAllocated, view: view };
    }
};
/**
 * @__sig ippp
 */
function _napi_get_array_length(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (!handle.isArray()) {
            return envObject.setLastError(8 /* napi_status.napi_array_expected */);
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var v = handle.value.length >>> 0;
        {{{ makeSetValue('result', 0, 'v', 'u32') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppp
 */
function _napi_get_arraybuffer_info(env, arraybuffer, data, byte_length) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!arraybuffer)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(arraybuffer);
    if (!handle.isArrayBuffer() && !emnapiExternalMemory.isSharedArrayBuffer(handle.value)) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    if (data) {
        {{{ from64('data') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var p = emnapiExternalMemory.getArrayBufferPointer(handle.value, true).address;
        {{{ makeSetValue('data', 0, 'p', '*') }}};
    }
    if (byte_length) {
        {{{ from64('byte_length') }}};
        {{{ makeSetValue('byte_length', 0, 'handle.value.byteLength', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _node_api_set_prototype(env, object, value) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var obj = emnapiCtx.handleStore.get(object).value;
        if (obj == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var type = typeof obj;
        var v = void 0;
        try {
            v = (type === 'object' && obj !== null) || type === 'function' ? obj : Object(obj);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        var val = emnapiCtx.handleStore.get(value).value;
        Object.setPrototypeOf(v, val);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippp
 */
function _napi_get_prototype(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (handle.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = handle.isObject() || handle.isFunction() ? handle.value : Object(handle.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var p = envObject.ensureHandleId(Object.getPrototypeOf(v));
        {{{ makeSetValue('result', 0, 'p', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippppppp
 */
function _napi_get_typedarray_info(env, typedarray, type, length, data, arraybuffer, byte_offset) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!typedarray)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(typedarray);
    if (!handle.isTypedArray()) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    var v = handle.value;
    if (type) {
        {{{ from64('type') }}};
        var t = void 0;
        if (v instanceof Int8Array) {
            t = 0 /* napi_typedarray_type.napi_int8_array */;
        }
        else if (v instanceof Uint8Array) {
            t = 1 /* napi_typedarray_type.napi_uint8_array */;
        }
        else if (v instanceof Uint8ClampedArray) {
            t = 2 /* napi_typedarray_type.napi_uint8_clamped_array */;
        }
        else if (v instanceof Int16Array) {
            t = 3 /* napi_typedarray_type.napi_int16_array */;
        }
        else if (v instanceof Uint16Array) {
            t = 4 /* napi_typedarray_type.napi_uint16_array */;
        }
        else if (v instanceof Int32Array) {
            t = 5 /* napi_typedarray_type.napi_int32_array */;
        }
        else if (v instanceof Uint32Array) {
            t = 6 /* napi_typedarray_type.napi_uint32_array */;
        }
        else if (typeof Float16Array === 'function' && v instanceof Float16Array) {
            t = 11 /* napi_typedarray_type.napi_float16_array */;
        }
        else if (v instanceof Float32Array) {
            t = 7 /* napi_typedarray_type.napi_float32_array */;
        }
        else if (v instanceof Float64Array) {
            t = 8 /* napi_typedarray_type.napi_float64_array */;
        }
        else if (v instanceof BigInt64Array) {
            t = 9 /* napi_typedarray_type.napi_bigint64_array */;
        }
        else if (v instanceof BigUint64Array) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            t = 10 /* napi_typedarray_type.napi_biguint64_array */;
        }
        else {
            return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
        }
        {{{ makeSetValue('type', 0, 't', 'i32') }}};
    }
    if (length) {
        {{{ from64('length') }}};
        {{{ makeSetValue('length', 0, 'v.length', SIZE_TYPE) }}};
    }
    var buffer;
    if (data || arraybuffer) {
        buffer = v.buffer;
        if (data) {
            {{{ from64('data') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var p = emnapiExternalMemory.getViewPointer(v, true).address;
            {{{ makeSetValue('data', 0, 'p', '*') }}};
        }
        if (arraybuffer) {
            {{{ from64('arraybuffer') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var ab = envObject.ensureHandleId(buffer);
            {{{ makeSetValue('arraybuffer', 0, 'ab', '*') }}};
        }
    }
    if (byte_offset) {
        {{{ from64('byte_offset') }}};
        {{{ makeSetValue('byte_offset', 0, 'v.byteOffset', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ipppp
 */
function _napi_get_buffer_info(env, buffer, data, length) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!buffer)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(buffer);
    if (!handle.isBuffer(emnapiCtx.feature.Buffer))
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (handle.isDataView()) {
        return _napi_get_dataview_info(env, buffer, length, data, 0, 0);
    }
    return _napi_get_typedarray_info(env, buffer, 0, length, data, 0, 0);
}
/**
 * @__sig ipppppp
 */
function _napi_get_dataview_info(env, dataview, byte_length, data, arraybuffer, byte_offset) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!dataview)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(dataview);
    if (!handle.isDataView()) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    var v = handle.value;
    if (byte_length) {
        {{{ from64('byte_length') }}};
        {{{ makeSetValue('byte_length', 0, 'v.byteLength', SIZE_TYPE) }}};
    }
    var buffer;
    if (data || arraybuffer) {
        buffer = v.buffer;
        if (data) {
            {{{ from64('data') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var p = emnapiExternalMemory.getViewPointer(v, true).address;
            {{{ makeSetValue('data', 0, 'p', '*') }}};
        }
        if (arraybuffer) {
            {{{ from64('arraybuffer') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var ab = envObject.ensureHandleId(buffer);
            {{{ makeSetValue('arraybuffer', 0, 'ab', '*') }}};
        }
    }
    if (byte_offset) {
        {{{ from64('byte_offset') }}};
        {{{ makeSetValue('byte_offset', 0, 'v.byteOffset', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_date_value(env, value, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (!handle.isDate()) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        {{{ from64('result') }}};
        v = handle.value.valueOf();
        {{{ makeSetValue('result', 0, 'v', 'double') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippp
 */
function _napi_get_value_bool(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'boolean') {
        return envObject.setLastError(7 /* napi_status.napi_boolean_expected */);
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = handle.value ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_value_double(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'number') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = handle.value;
    {{{ makeSetValue('result', 0, 'r', 'double') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipppp
 */
function _napi_get_value_bigint_int64(env, value, result, lossless) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportBigInt) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!lossless)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    var numberValue = handle.value;
    if (typeof numberValue !== 'bigint') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    {{{ from64('lossless') }}};
    {{{ from64('result') }}};
    if ((numberValue >= (BigInt(-1) * (BigInt(1) << BigInt(63)))) && (numberValue < (BigInt(1) << BigInt(63)))) {
        {{{ makeSetValue('lossless', 0, '1', 'i8') }}};
    }
    else {
        {{{ makeSetValue('lossless', 0, '0', 'i8') }}};
        numberValue = numberValue & ((BigInt(1) << BigInt(64)) - BigInt(1));
        if (numberValue >= (BigInt(1) << BigInt(63))) {
            numberValue = numberValue - (BigInt(1) << BigInt(64));
        }
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var low = Number(numberValue & BigInt(0xffffffff));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var high = Number(numberValue >> BigInt(32));
    {{{ makeSetValue('result', 0, 'low', 'i32') }}};
    {{{ makeSetValue('result', 4, 'high', 'i32') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipppp
 */
function _napi_get_value_bigint_uint64(env, value, result, lossless) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportBigInt) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!lossless)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    var numberValue = handle.value;
    if (typeof numberValue !== 'bigint') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    {{{ from64('lossless') }}};
    {{{ from64('result') }}};
    if ((numberValue >= BigInt(0)) && (numberValue < (BigInt(1) << BigInt(64)))) {
        {{{ makeSetValue('lossless', 0, '1', 'i8') }}};
    }
    else {
        {{{ makeSetValue('lossless', 0, '0', 'i8') }}};
        numberValue = numberValue & ((BigInt(1) << BigInt(64)) - BigInt(1));
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var low = Number(numberValue & BigInt(0xffffffff));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var high = Number(numberValue >> BigInt(32));
    {{{ makeSetValue('result', 0, 'low', 'u32') }}};
    {{{ makeSetValue('result', 4, 'high', 'u32') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippppp
 */
function _napi_get_value_bigint_words(env, value, sign_bit, word_count, words) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportBigInt) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!word_count)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (!handle.isBigInt()) {
        return envObject.setLastError(17 /* napi_status.napi_bigint_expected */);
    }
    var isMinus = handle.value < BigInt(0);
    {{{ from64('sign_bit') }}};
    {{{ from64('words') }}};
    {{{ from64('word_count') }}};
    var word_count_int = {{{ makeGetValue('word_count', 0, SIZE_TYPE) }}};
    {{{ from64('word_count_int') }}};
    var wordCount = 0;
    var bigintValue = isMinus ? (handle.value * BigInt(-1)) : handle.value;
    while (bigintValue !== BigInt(0)) {
        wordCount++;
        bigintValue = bigintValue >> BigInt(64);
    }
    bigintValue = isMinus ? (handle.value * BigInt(-1)) : handle.value;
    if (!sign_bit && !words) {
        word_count_int = wordCount;
        {{{ makeSetValue('word_count', 0, 'word_count_int', SIZE_TYPE) }}};
    }
    else {
        if (!sign_bit)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!words)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var wordsArr = [];
        while (bigintValue !== BigInt(0)) {
            var uint64 = bigintValue & ((BigInt(1) << BigInt(64)) - BigInt(1));
            wordsArr.push(uint64);
            bigintValue = bigintValue >> BigInt(64);
        }
        var len = Math.min(word_count_int, wordsArr.length);
        for (var i = 0; i < len; i++) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var low = Number(wordsArr[i] & BigInt(0xffffffff));
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var high = Number(wordsArr[i] >> BigInt(32));
            {{{ makeSetValue('words', 'i * 8', 'low', 'u32') }}};
            {{{ makeSetValue('words', 'i * 8 + 4', 'high', 'u32') }}};
        }
        {{{ makeSetValue('sign_bit', 0, 'isMinus ? 1 : 0', 'i32') }}};
        {{{ makeSetValue('word_count', 0, 'len', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_value_external(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (!handle.isExternal()) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var p = handle.data();
    {{{ makeSetValue('result', 0, 'p', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_value_int32(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'number') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = new Int32Array([handle.value])[0];
    {{{ makeSetValue('result', 0, 'v', 'i32') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_value_int64(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'number') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    var numberValue = handle.value;
    {{{ from64('result') }}};
    if (numberValue === Number.POSITIVE_INFINITY || numberValue === Number.NEGATIVE_INFINITY || isNaN(numberValue)) {
        {{{ makeSetValue('result', 0, '0', 'i32') }}};
        {{{ makeSetValue('result', 4, '0', 'i32') }}};
    }
    else if (numberValue < /* INT64_RANGE_NEGATIVE */ -9223372036854776000) {
        {{{ makeSetValue('result', 0, '0', 'i32') }}};
        {{{ makeSetValue('result', 4, '0x80000000', 'i32') }}};
    }
    else if (numberValue >= /* INT64_RANGE_POSITIVE */ 9223372036854776000) {
        {{{ makeSetValue('result', 0, '0xffffffff', 'u32') }}};
        {{{ makeSetValue('result', 4, '0x7fffffff', 'u32') }}};
    }
    else {
        emnapiSetValueI64(result, Math.trunc(numberValue));
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippppp
 */
function _napi_get_value_string_latin1(env, value, buf, buf_size, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    {{{ from64('buf') }}};
    {{{ from64('buf_size') }}};
    buf_size = buf_size >>> 0;
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    if (!buf) {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ makeSetValue('result', 0, 'handle.value.length', SIZE_TYPE) }}};
    }
    else if (buf_size !== 0) {
        var copied = 0;
        var v = void 0;
        for (var i = 0; i < buf_size - 1; ++i) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            v = handle.value.charCodeAt(i) & 0xff;
            {{{ makeSetValue('buf', 'i', 'v', 'u8') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            copied++;
        }
        {{{ makeSetValue('buf', 'copied', '0', 'u8') }}};
        if (result) {
            {{{ makeSetValue('result', 0, 'copied', SIZE_TYPE) }}};
        }
    }
    else if (result) {
        {{{ makeSetValue('result', 0, '0', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippppp
 */
function _napi_get_value_string_utf8(env, value, buf, buf_size, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    {{{ from64('buf') }}};
    {{{ from64('buf_size') }}};
    buf_size = buf_size >>> 0;
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    if (!buf) {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var strLength = emnapiString.lengthBytesUTF8(handle.value);
        {{{ makeSetValue('result', 0, 'strLength', SIZE_TYPE) }}};
    }
    else if (buf_size !== 0) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var copied = emnapiString.stringToUTF8(handle.value, buf, buf_size);
        if (result) {
            {{{ makeSetValue('result', 0, 'copied', SIZE_TYPE) }}};
        }
    }
    else if (result) {
        {{{ makeSetValue('result', 0, '0', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippppp
 */
function _napi_get_value_string_utf16(env, value, buf, buf_size, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    {{{ from64('buf') }}};
    {{{ from64('buf_size') }}};
    buf_size = buf_size >>> 0;
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    if (!buf) {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ makeSetValue('result', 0, 'handle.value.length', SIZE_TYPE) }}};
    }
    else if (buf_size !== 0) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var copied = emnapiString.stringToUTF16(handle.value, buf, buf_size * 2);
        if (result) {
            {{{ makeSetValue('result', 0, 'copied / 2', SIZE_TYPE) }}};
        }
    }
    else if (result) {
        {{{ makeSetValue('result', 0, '0', SIZE_TYPE) }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_get_value_uint32(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (typeof handle.value !== 'number') {
        return envObject.setLastError(6 /* napi_status.napi_number_expected */);
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = new Uint32Array([handle.value])[0];
    {{{ makeSetValue('result', 0, 'v', 'u32') }}};
    return envObject.clearLastError();
}
/* eslint-disable @typescript-eslint/indent */
/**
 * @__sig ipip
 */
function _napi_create_int32(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = emnapiCtx.addToCurrentScope(value).id;
    {{{ makeSetValue('result', 0, 'v', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipip
 */
function _napi_create_uint32(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = emnapiCtx.addToCurrentScope(value >>> 0).id;
    {{{ makeSetValue('result', 0, 'v', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipjp
 */
function _napi_create_int64(env, low, high, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    var value;
#if WASM_BIGINT
    if (!high)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = Number(low);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v1 = emnapiCtx.addToCurrentScope(value).id;
    {{{ from64('high') }}};
    {{{ makeSetValue('high', 0, 'v1', '*') }}};
#else
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = (low >>> 0) + (high * Math.pow(2, 32));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v2 = emnapiCtx.addToCurrentScope(value).id;
    {{{ makeSetValue('result', 0, 'v2', '*') }}};
#endif
    return envObject.clearLastError();
}
/**
 * @__sig ipdp
 */
function _napi_create_double(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = emnapiCtx.addToCurrentScope(value).id;
    {{{ makeSetValue('result', 0, 'v', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipppp
 */
function _napi_create_string_latin1(env, str, length, result) {
    return emnapiString.newString(env, str, length, result, function (str, autoLength, sizeLength) {
        var latin1String = '';
        var len = 0;
        if (autoLength) {
            while (true) {
                var ch = {{{ makeGetValue('str', 0, 'u8') }}};
                if (!ch)
                    break;
                latin1String += String.fromCharCode(ch);
                str++;
            }
        }
        else {
            while (len < sizeLength) {
                var ch = {{{ makeGetValue('str', 0, 'u8') }}};
                if (!ch)
                    break;
                latin1String += String.fromCharCode(ch);
                len++;
                str++;
            }
        }
        return latin1String;
    });
}
/**
 * @__sig ipppp
 */
function _napi_create_string_utf16(env, str, length, result) {
    return emnapiString.newString(env, str, length, result, function (str) {
        return emnapiString.UTF16ToString(str, length);
    });
}
/**
 * @__sig ipppp
 */
function _napi_create_string_utf8(env, str, length, result) {
    return emnapiString.newString(env, str, length, result, function (str) {
        return emnapiString.UTF8ToString(str, length);
    });
}
/**
 * @__sig ippppppp
 */
function _node_api_create_external_string_latin1(env, str, length, finalize_callback, finalize_hint, result, copied) {
    return emnapiString.newExternalString(env, str, length, finalize_callback, finalize_hint, result, copied, _napi_create_string_latin1, undefined);
}
/**
 * @__sig ippppppp
 */
function _node_api_create_external_string_utf16(env, str, length, finalize_callback, finalize_hint, result, copied) {
    return emnapiString.newExternalString(env, str, length, finalize_callback, finalize_hint, result, copied, _napi_create_string_utf16, undefined);
}
/**
 * @__sig ipppp
 */
function _node_api_create_property_key_latin1(env, str, length, result) {
    return _napi_create_string_latin1(env, str, length, result);
}
/**
 * @__sig ipppp
 */
function _node_api_create_property_key_utf8(env, str, length, result) {
    return _napi_create_string_utf8(env, str, length, result);
}
/**
 * @__sig ipppp
 */
function _node_api_create_property_key_utf16(env, str, length, result) {
    return _napi_create_string_utf16(env, str, length, result);
}
/**
 * @__sig ipjp
 */
function _napi_create_bigint_int64(env, low, high, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportBigInt) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    var value;
#if WASM_BIGINT
    if (!high)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = low;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v1 = emnapiCtx.addToCurrentScope(value).id;
    {{{ from64('high') }}};
    {{{ makeSetValue('high', 0, 'v1', '*') }}};
#else
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = BigInt(low >>> 0) | (BigInt(high) << BigInt(32));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v2 = emnapiCtx.addToCurrentScope(value).id;
    {{{ makeSetValue('result', 0, 'v2', '*') }}};
#endif
    return envObject.clearLastError();
}
/**
 * @__sig ipjp
 */
function _napi_create_bigint_uint64(env, low, high, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportBigInt) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    var value;
#if WASM_BIGINT
    if (!high)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = low & ((BigInt(1) << BigInt(64)) - BigInt(1));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v1 = emnapiCtx.addToCurrentScope(value).id;
    {{{ from64('high') }}};
    {{{ makeSetValue('high', 0, 'v1', '*') }}};
#else
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    value = BigInt(low >>> 0) | (BigInt(high >>> 0) << BigInt(32));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v2 = emnapiCtx.addToCurrentScope(value).id;
    {{{ makeSetValue('result', 0, 'v2', '*') }}};
#endif
    return envObject.clearLastError();
}
/**
 * @__sig ipippp
 */
function _napi_create_bigint_words(env, sign_bit, word_count, words, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v, i;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!emnapiCtx.feature.supportBigInt) {
            return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
        }
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('words') }}};
        {{{ from64('word_count') }}};
        word_count = word_count >>> 0;
        if (word_count > 2147483647) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (word_count > (1024 * 1024 / (4 * 8) / 2)) {
            throw new RangeError('Maximum BigInt size exceeded');
        }
        var value = BigInt(0);
        for (i = 0; i < word_count; i++) {
            var low = {{{ makeGetValue('words', 'i * 8', 'u32') }}};
            var high = {{{ makeGetValue('words', 'i * 8 + 4', 'u32') }}};
            var wordi = BigInt(low) | (BigInt(high) << BigInt(32));
            value += wordi << BigInt(64 * i);
        }
        value *= ((BigInt(sign_bit) % BigInt(2) === BigInt(0)) ? BigInt(1) : BigInt(-1));
        {{{ from64('result') }}};
        v = emnapiCtx.addToCurrentScope(value).id;
        {{{ makeSetValue('result', 0, 'v', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/* eslint-disable @typescript-eslint/indent */
/* eslint-disable no-new-func */
/* eslint-disable @typescript-eslint/no-implied-eval */
function emnapiCreateFunction(envObject, utf8name, length, cb, data) {
    {{{ from64('utf8name') }}};
    var functionName = (!utf8name || !length) ? '' : (emnapiString.UTF8ToString(utf8name, length));
    var f;
    var napiCallback = {{{ makeDynCall('ppp', 'cb') }}};
    var callback = function (envObject) {
        return napiCallback(envObject.id, envObject.ctx.scopeStore.currentScope.id);
    };
    var makeFunction = function (envObject, callback) {
        return function () {
            'use strict';
            var scope = envObject.ctx.openScope(envObject);
            var callbackInfo = scope.callbackInfo;
            callbackInfo.data = data;
            callbackInfo.args = arguments;
            callbackInfo.thiz = this;
            callbackInfo.fn = f;
            try {
                var napiValue = envObject.callIntoModule(callback);
                return (!napiValue) ? undefined : envObject.ctx.handleStore.get(napiValue).value;
            }
            finally {
                callbackInfo.data = 0;
                callbackInfo.args = undefined;
                callbackInfo.thiz = undefined;
                callbackInfo.fn = undefined;
                envObject.ctx.closeScope(envObject, scope);
            }
        };
    };
    if (functionName === '') {
        f = makeFunction(envObject, callback);
        return { status: 0 /* napi_status.napi_ok */, f: f };
    }
    if (!(/^[_$a-zA-Z][_$a-zA-Z0-9]*$/.test(functionName))) {
        return { status: 1 /* napi_status.napi_invalid_arg */, f: undefined };
    }
#if DYNAMIC_EXECUTION
    if (emnapiCtx.feature.supportNewFunction) {
        var _ = makeFunction(envObject, callback);
        try {
            f = (new Function('_', 'return function ' + functionName + '(){' +
                '"use strict";' +
                'return _.apply(this,arguments);' +
                '};'))(_);
        }
        catch (_err) {
            f = makeFunction(envObject, callback);
            if (emnapiCtx.feature.canSetFunctionName)
                Object.defineProperty(f, 'name', { value: functionName });
        }
    }
    else {
        f = makeFunction(envObject, callback);
        if (emnapiCtx.feature.canSetFunctionName)
            Object.defineProperty(f, 'name', { value: functionName });
    }
#else
    f = makeFunction(envObject, callback);
    if (emnapiCtx.feature.canSetFunctionName)
        Object.defineProperty(f, 'name', { value: functionName });
#endif
    return { status: 0 /* napi_status.napi_ok */, f: f };
}
function emnapiDefineProperty(envObject, obj, propertyName, method, getter, setter, value, attributes, data) {
    if (getter || setter) {
        var localGetter = void 0;
        var localSetter = void 0;
        if (getter) {
            localGetter = emnapiCreateFunction(envObject, 0, 0, getter, data).f;
        }
        if (setter) {
            localSetter = emnapiCreateFunction(envObject, 0, 0, setter, data).f;
        }
        var desc = {
            configurable: (attributes & 4 /* napi_property_attributes.napi_configurable */) !== 0,
            enumerable: (attributes & 2 /* napi_property_attributes.napi_enumerable */) !== 0,
            get: localGetter,
            set: localSetter
        };
        Object.defineProperty(obj, propertyName, desc);
    }
    else if (method) {
        var localMethod = emnapiCreateFunction(envObject, 0, 0, method, data).f;
        var desc = {
            configurable: (attributes & 4 /* napi_property_attributes.napi_configurable */) !== 0,
            enumerable: (attributes & 2 /* napi_property_attributes.napi_enumerable */) !== 0,
            writable: (attributes & 1 /* napi_property_attributes.napi_writable */) !== 0,
            value: localMethod
        };
        Object.defineProperty(obj, propertyName, desc);
    }
    else {
        var desc = {
            configurable: (attributes & 4 /* napi_property_attributes.napi_configurable */) !== 0,
            enumerable: (attributes & 2 /* napi_property_attributes.napi_enumerable */) !== 0,
            writable: (attributes & 1 /* napi_property_attributes.napi_writable */) !== 0,
            value: emnapiCtx.handleStore.get(value).value
        };
        Object.defineProperty(obj, propertyName, desc);
    }
}
function emnapiGetHandle(js_object) {
    var handle = emnapiCtx.handleStore.get(js_object);
    if (!(handle.isObject() || handle.isFunction())) {
        return { status: 1 /* napi_status.napi_invalid_arg */ };
    }
    if (typeof emnapiExternalMemory !== 'undefined' && ArrayBuffer.isView(handle.value)) {
        if (emnapiExternalMemory.wasmMemoryViewTable.has(handle.value)) {
            handle = emnapiCtx.addToCurrentScope(emnapiExternalMemory.wasmMemoryViewTable.get(handle.value));
        }
    }
    return { status: 0 /* napi_status.napi_ok */, handle: handle };
}
function emnapiWrap(env, js_object, native_object, finalize_cb, finalize_hint, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var referenceId;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!emnapiCtx.feature.supportFinalizer) {
            if (finalize_cb) {
                throw emnapiCtx.createNotSupportWeakRefError('napi_wrap', 'Parameter "finalize_cb" must be 0(NULL)');
            }
            if (result) {
                throw emnapiCtx.createNotSupportWeakRefError('napi_wrap', 'Parameter "result" must be 0(NULL)');
            }
        }
        if (!js_object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handleResult = emnapiGetHandle(js_object);
        if (handleResult.status !== 0 /* napi_status.napi_ok */) {
            return envObject.setLastError(handleResult.status);
        }
        var handle = handleResult.handle;
        if (envObject.getObjectBinding(handle.value).wrapped !== 0) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var reference = void 0;
        if (result) {
            if (!finalize_cb)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
            reference = emnapiCtx.createReferenceWithFinalizer(envObject, handle.id, 0, 1 /* ReferenceOwnership.kUserland */, finalize_cb, native_object, finalize_hint);
            {{{ from64('result') }}};
            referenceId = reference.id;
            {{{ makeSetValue('result', 0, 'referenceId', '*') }}};
        }
        else if (finalize_cb) {
            reference = emnapiCtx.createReferenceWithFinalizer(envObject, handle.id, 0, 0 /* ReferenceOwnership.kRuntime */, finalize_cb, native_object, finalize_hint);
        }
        else {
            reference = emnapiCtx.createReferenceWithData(envObject, handle.id, 0, 0 /* ReferenceOwnership.kRuntime */, native_object);
        }
        envObject.getObjectBinding(handle.value).wrapped = reference.id;
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
function emnapiUnwrap(env, js_object, result, action) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var data;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!js_object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (action === 0 /* UnwrapAction.KeepWrap */) {
            if (!result)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var value = emnapiCtx.handleStore.get(js_object);
        if (!(value.isObject() || value.isFunction())) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var binding = envObject.getObjectBinding(value.value);
        var referenceId = binding.wrapped;
        var ref = emnapiCtx.refStore.get(referenceId);
        if (!ref)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (result) {
            {{{ from64('result') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            data = ref.data();
            {{{ makeSetValue('result', 0, 'data', '*') }}};
        }
        if (action === 1 /* UnwrapAction.RemoveWrap */) {
            binding.wrapped = 0;
            if (ref.ownership() === 1 /* ReferenceOwnership.kUserland */) {
                // When the wrap is been removed, the finalizer should be reset.
                ref.resetFinalizer();
            }
            else {
                ref.dispose();
            }
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppppppp
 */
function _napi_define_class(env, utf8name, length, constructor, callback_data, property_count, properties, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var propPtr, valueHandleId, attributes;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!constructor)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('length') }}};
        {{{ from64('properties') }}};
        {{{ from64('property_count') }}};
        property_count = property_count >>> 0;
        if (property_count > 0) {
            if (!properties)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if ((length < -1) || (length > 2147483647) || (!utf8name)) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var fresult = emnapiCreateFunction(envObject, utf8name, length, constructor, callback_data);
        if (fresult.status !== 0 /* napi_status.napi_ok */)
            return envObject.setLastError(fresult.status);
        var F = fresult.f;
        var propertyName = void 0;
        for (var i = 0; i < property_count; i++) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            propPtr = properties + (i * ({{{ POINTER_SIZE }}} * 8));
            var utf8Name = {{{ makeGetValue('propPtr', 0, '*') }}};
            var name_1 = {{{ makeGetValue('propPtr', POINTER_SIZE, '*') }}};
            var method = {{{ makeGetValue('propPtr', POINTER_SIZE * 2, '*') }}};
            var getter = {{{ makeGetValue('propPtr', POINTER_SIZE * 3, '*') }}};
            var setter = {{{ makeGetValue('propPtr', POINTER_SIZE * 4, '*') }}};
            var value = {{{ makeGetValue('propPtr', POINTER_SIZE * 5, '*') }}};
            attributes = {{{ makeGetValue('propPtr', POINTER_SIZE * 6, POINTER_WASM_TYPE) }}};
            {{{ from64('attributes') }}};
            var data = {{{ makeGetValue('propPtr', POINTER_SIZE * 7, '*') }}};
            if (utf8Name) {
                propertyName = emnapiString.UTF8ToString(utf8Name, -1);
            }
            else {
                if (!name_1) {
                    return envObject.setLastError(4 /* napi_status.napi_name_expected */);
                }
                propertyName = emnapiCtx.handleStore.get(name_1).value;
                if (typeof propertyName !== 'string' && typeof propertyName !== 'symbol') {
                    return envObject.setLastError(4 /* napi_status.napi_name_expected */);
                }
            }
            if ((attributes & 1024 /* napi_property_attributes.napi_static */) !== 0) {
                emnapiDefineProperty(envObject, F, propertyName, method, getter, setter, value, attributes, data);
                continue;
            }
            emnapiDefineProperty(envObject, F.prototype, propertyName, method, getter, setter, value, attributes, data);
        }
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var valueHandle = emnapiCtx.addToCurrentScope(F);
        valueHandleId = valueHandle.id;
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'valueHandleId', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppppp
 */
function _napi_wrap(env, js_object, native_object, finalize_cb, finalize_hint, result) {
    return emnapiWrap(env, js_object, native_object, finalize_cb, finalize_hint, result);
}
/**
 * @__sig ippp
 */
function _napi_unwrap(env, js_object, result) {
    return emnapiUnwrap(env, js_object, result, 0 /* UnwrapAction.KeepWrap */);
}
/**
 * @__sig ippp
 */
function _napi_remove_wrap(env, js_object, result) {
    return emnapiUnwrap(env, js_object, result, 1 /* UnwrapAction.RemoveWrap */);
}
/**
 * @__sig ippp
 */
function _napi_type_tag_object(env, object, type_tag) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        var value = emnapiCtx.handleStore.get(object);
        if (!(value.isObject() || value.isFunction())) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('type_tag') }}};
        if (!type_tag) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        var binding = envObject.getObjectBinding(value.value);
        if (binding.tag !== null) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        var tag = new Uint8Array(16);
        tag.set(new Uint8Array(wasmMemory.buffer, type_tag, 16));
        binding.tag = new Uint32Array(tag.buffer);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppp
 */
function _napi_check_object_type_tag(env, object, type_tag, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars, one-var
    var ret = true, i;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        var value = emnapiCtx.handleStore.get(object);
        if (!(value.isObject() || value.isFunction())) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 2 /* napi_status.napi_object_expected */);
        }
        if (!type_tag) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        if (!result) {
            return envObject.setLastError(envObject.tryCatch.hasCaught() ? 10 /* napi_status.napi_pending_exception */ : 1 /* napi_status.napi_invalid_arg */);
        }
        var binding = envObject.getObjectBinding(value.value);
        if (binding.tag !== null) {
            {{{ from64('type_tag') }}};
            var tag = binding.tag;
            var typeTag = new Uint32Array(wasmMemory.buffer, type_tag, 4);
            ret = (tag[0] === typeTag[0] &&
                tag[1] === typeTag[1] &&
                tag[2] === typeTag[2] &&
                tag[3] === typeTag[3]);
        }
        else {
            ret = false;
        }
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'ret ? 1 : 0', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppppp
 */
function _napi_add_finalizer(env, js_object, finalize_data, finalize_cb, finalize_hint, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!emnapiCtx.feature.supportFinalizer) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    if (!js_object)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!finalize_cb)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handleResult = emnapiGetHandle(js_object);
    if (handleResult.status !== 0 /* napi_status.napi_ok */) {
        return envObject.setLastError(handleResult.status);
    }
    var handle = handleResult.handle;
    var ownership = !result ? 0 /* ReferenceOwnership.kRuntime */ : 1 /* ReferenceOwnership.kUserland */;
    {{{ from64('finalize_data') }}};
    {{{ from64('finalize_cb') }}};
    {{{ from64('finalize_hint') }}};
    var reference = emnapiCtx.createReferenceWithFinalizer(envObject, handle.id, 0, ownership, finalize_cb, finalize_data, finalize_hint);
    if (result) {
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var referenceId = reference.id;
        {{{ makeSetValue('result', 0, 'referenceId', '*') }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ipppp
 */
function _node_api_post_finalizer(env, finalize_cb, finalize_data, finalize_hint) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    envObject.enqueueFinalizer(emnapiCtx.createTrackedFinalizer(envObject, finalize_cb, finalize_data, finalize_hint));
    return envObject.clearLastError();
}
/**
 * @__sig ipippppp
 */
function _emnapi_create_memory_view(env, typedarray_type, external_data, byte_length, finalize_cb, finalize_hint, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('byte_length') }}};
        {{{ from64('external_data') }}};
        {{{ from64('result') }}};
        byte_length = byte_length >>> 0;
        if (!external_data) {
            byte_length = 0;
        }
        if (byte_length > 2147483647) {
            throw new RangeError('Cannot create a memory view larger than 2147483647 bytes');
        }
        if ((external_data + byte_length) > wasmMemory.buffer.byteLength) {
            throw new RangeError('Memory out of range');
        }
        if (!emnapiCtx.feature.supportFinalizer && finalize_cb) {
            throw emnapiCtx.createNotSupportWeakRefError('emnapi_create_memory_view', 'Parameter "finalize_cb" must be 0(NULL)');
        }
        var viewDescriptor = void 0;
        switch (typedarray_type) {
            case 0 /* emnapi_memory_view_type.emnapi_int8_array */:
                viewDescriptor = { Ctor: Int8Array, address: external_data, length: byte_length, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 1 /* emnapi_memory_view_type.emnapi_uint8_array */:
                viewDescriptor = { Ctor: Uint8Array, address: external_data, length: byte_length, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 2 /* emnapi_memory_view_type.emnapi_uint8_clamped_array */:
                viewDescriptor = { Ctor: Uint8ClampedArray, address: external_data, length: byte_length, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 3 /* emnapi_memory_view_type.emnapi_int16_array */:
                viewDescriptor = { Ctor: Int16Array, address: external_data, length: byte_length >> 1, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 4 /* emnapi_memory_view_type.emnapi_uint16_array */:
                viewDescriptor = { Ctor: Uint16Array, address: external_data, length: byte_length >> 1, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 5 /* emnapi_memory_view_type.emnapi_int32_array */:
                viewDescriptor = { Ctor: Int32Array, address: external_data, length: byte_length >> 2, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 6 /* emnapi_memory_view_type.emnapi_uint32_array */:
                viewDescriptor = { Ctor: Uint32Array, address: external_data, length: byte_length >> 2, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 7 /* emnapi_memory_view_type.emnapi_float32_array */:
                viewDescriptor = { Ctor: Float32Array, address: external_data, length: byte_length >> 2, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 8 /* emnapi_memory_view_type.emnapi_float64_array */:
                viewDescriptor = { Ctor: Float64Array, address: external_data, length: byte_length >> 3, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 9 /* emnapi_memory_view_type.emnapi_bigint64_array */:
                viewDescriptor = { Ctor: BigInt64Array, address: external_data, length: byte_length >> 3, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 10 /* emnapi_memory_view_type.emnapi_biguint64_array */:
                viewDescriptor = { Ctor: BigUint64Array, address: external_data, length: byte_length >> 3, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case -1 /* emnapi_memory_view_type.emnapi_data_view */:
                viewDescriptor = { Ctor: DataView, address: external_data, length: byte_length, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case 11 /* emnapi_memory_view_type.emnapi_float16_array */:
                if (typeof Float16Array !== 'function') {
                    return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
                }
                viewDescriptor = { Ctor: Float16Array, address: external_data, length: byte_length >> 1, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            case -2 /* emnapi_memory_view_type.emnapi_buffer */: {
                if (!emnapiCtx.feature.Buffer) {
                    throw emnapiCtx.createNotSupportBufferError('emnapi_create_memory_view', '');
                }
                viewDescriptor = { Ctor: emnapiCtx.feature.Buffer, address: external_data, length: byte_length, ownership: 1 /* ReferenceOwnership.kUserland */, runtimeAllocated: 0 };
                break;
            }
            default: return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var Ctor = viewDescriptor.Ctor;
        var typedArray = typedarray_type === -2 /* emnapi_memory_view_type.emnapi_buffer */
            ? emnapiCtx.feature.Buffer.from(wasmMemory.buffer, viewDescriptor.address, viewDescriptor.length)
            : new Ctor(wasmMemory.buffer, viewDescriptor.address, viewDescriptor.length);
        var handle = emnapiCtx.addToCurrentScope(typedArray);
        emnapiExternalMemory.wasmMemoryViewTable.set(typedArray, viewDescriptor);
        if (finalize_cb) {
            var status_1 = _napi_add_finalizer(env, handle.id, external_data, finalize_cb, finalize_hint, /* NULL */ 0);
            if (status_1 === 10 /* napi_status.napi_pending_exception */) {
                var err = envObject.tryCatch.extractException();
                envObject.clearLastError();
                throw err;
            }
            else if (status_1 !== 0 /* napi_status.napi_ok */) {
                return envObject.setLastError(status_1);
            }
        }
        value = handle.id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig i
 */
function _emnapi_is_support_weakref() {
    return emnapiCtx.feature.supportFinalizer ? 1 : 0;
}
/**
 * @__sig i
 */
function _emnapi_is_support_bigint() {
    return emnapiCtx.feature.supportBigInt ? 1 : 0;
}
/**
 * @__sig i
 */
function _emnapi_is_node_binding_available() {
    return emnapiNodeBinding ? 1 : 0;
}
function emnapiSyncMemory(js_to_wasm, arrayBufferOrView, offset, len) {
    offset = offset !== null && offset !== void 0 ? offset : 0;
    offset = offset >>> 0;
    var view;
    if (arrayBufferOrView instanceof ArrayBuffer || emnapiExternalMemory.isSharedArrayBuffer(arrayBufferOrView)) {
        var pointer = emnapiExternalMemory.getArrayBufferPointer(arrayBufferOrView, false).address;
        if (!pointer)
            throw new Error('Unknown ArrayBuffer address');
        if (typeof len !== 'number' || len === -1) {
            len = arrayBufferOrView.byteLength - offset;
        }
        len = len >>> 0;
        if (len === 0)
            return arrayBufferOrView;
        view = new Uint8Array(arrayBufferOrView, offset, len);
        var wasmMemoryU8 = new Uint8Array(wasmMemory.buffer);
        if (!js_to_wasm) {
            view.set(wasmMemoryU8.subarray(pointer, pointer + len));
        }
        else {
            wasmMemoryU8.set(view, pointer);
        }
        return arrayBufferOrView;
    }
    if (ArrayBuffer.isView(arrayBufferOrView)) {
        var viewPointerInfo = emnapiExternalMemory.getViewPointer(arrayBufferOrView, false);
        var latestView = viewPointerInfo.view;
        var pointer = viewPointerInfo.address;
        if (!pointer)
            throw new Error('Unknown ArrayBuffer address');
        if (typeof len !== 'number' || len === -1) {
            len = latestView.byteLength - offset;
        }
        len = len >>> 0;
        if (len === 0)
            return latestView;
        view = new Uint8Array(latestView.buffer, latestView.byteOffset + offset, len);
        var wasmMemoryU8 = new Uint8Array(wasmMemory.buffer);
        if (!js_to_wasm) {
            view.set(wasmMemoryU8.subarray(pointer, pointer + len));
        }
        else {
            wasmMemoryU8.set(view, pointer);
        }
        return latestView;
    }
    throw new TypeError('emnapiSyncMemory expect ArrayBuffer or ArrayBufferView as first parameter');
}
/**
 * @__sig ipippp
 */
function _emnapi_sync_memory(env, js_to_wasm, arraybuffer_or_view, offset, len) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer_or_view)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('arraybuffer_or_view') }}};
        {{{ from64('offset') }}};
        {{{ from64('len') }}};
        var handleId = {{{ makeGetValue('arraybuffer_or_view', 0, '*') }}};
        var handle = envObject.ctx.handleStore.get(handleId);
        if (!handle.isArrayBuffer() && !handle.isTypedArray() && !handle.isDataView() && !emnapiExternalMemory.isSharedArrayBuffer(handle.value)) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var ret = emnapiSyncMemory(Boolean(js_to_wasm), handle.value, offset, len);
        if (handle.value !== ret) {
            {{{ from64('arraybuffer_or_view') }}};
            v = envObject.ensureHandleId(ret);
            {{{ makeSetValue('arraybuffer_or_view', 0, 'v', '*') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
function emnapiGetMemoryAddress(arrayBufferOrView) {
    var isArrayBuffer = arrayBufferOrView instanceof ArrayBuffer;
    var isDataView = arrayBufferOrView instanceof DataView;
    var isTypedArray = ArrayBuffer.isView(arrayBufferOrView) && !isDataView;
    if (!isArrayBuffer && !isTypedArray && !isDataView && !emnapiExternalMemory.isSharedArrayBuffer(arrayBufferOrView)) {
        throw new TypeError('emnapiGetMemoryAddress expect ArrayBuffer or ArrayBufferView as first parameter');
    }
    var info;
    if (isArrayBuffer) {
        info = emnapiExternalMemory.getArrayBufferPointer(arrayBufferOrView, false);
    }
    else {
        info = emnapiExternalMemory.getViewPointer(arrayBufferOrView, false);
    }
    return {
        address: info.address,
        ownership: info.ownership,
        runtimeAllocated: info.runtimeAllocated
    };
}
/**
 * @__sig ipppp
 */
function _emnapi_get_memory_address(env, arraybuffer_or_view, address, ownership, runtime_allocated) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var p, runtimeAllocated, ownershipOut;
    var info;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer_or_view)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!address && !ownership && !runtime_allocated) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var handle = envObject.ctx.handleStore.get(arraybuffer_or_view);
        info = emnapiGetMemoryAddress(handle.value);
        p = info.address;
        if (address) {
            {{{ from64('address') }}};
            {{{ makeSetValue('address', 0, 'p', '*') }}};
        }
        if (ownership) {
            {{{ from64('ownership') }}};
            ownershipOut = info.ownership;
            {{{ makeSetValue('ownership', 0, 'ownershipOut', 'i32') }}};
        }
        if (runtime_allocated) {
            {{{ from64('runtime_allocated') }}};
            runtimeAllocated = info.runtimeAllocated;
            {{{ makeSetValue('runtime_allocated', 0, 'runtimeAllocated', 'i8') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipp
 */
function _emnapi_get_runtime_version(env, version) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!version)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var runtimeVersion;
    try {
        runtimeVersion = emnapiCtx.getRuntimeVersions().version;
    }
    catch (_) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var versions = runtimeVersion.split('.')
        .map(function (n) { return Number(n); });
    {{{ from64('version') }}};
    {{{ makeSetValue('version', 0, 'versions[0]', 'u32') }}};
    {{{ makeSetValue('version', 4, 'versions[1]', 'u32') }}};
    {{{ makeSetValue('version', 8, 'versions[2]', 'u32') }}};
    return envObject.clearLastError();
}
// emnapiImplementHelper('$emnapiSyncMemory', undefined, emnapiSyncMemory, ['$emnapiExternalMemory'], 'syncMemory')
// emnapiImplementHelper('$emnapiGetMemoryAddress', undefined, emnapiGetMemoryAddress, ['$emnapiExternalMemory'], 'getMemoryAddress')
/**
 * @__sig ipp
 */
function _napi_create_array(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope([]).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_create_array_with_length(env, length, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('length') }}};
    {{{ from64('result') }}};
    length = length >>> 0;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(new Array(length)).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
function emnapiCreateArrayBuffer(byte_length, data, shared) {
    {{{ from64('byte_length') }}};
    byte_length = byte_length >>> 0;
    var arrayBuffer = shared ? new SharedArrayBuffer(byte_length) : new ArrayBuffer(byte_length);
    if (data) {
        {{{ from64('data') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var p = emnapiExternalMemory.getArrayBufferPointer(arrayBuffer, true).address;
        {{{ makeSetValue('data', 0, 'p', '*') }}};
    }
    return arrayBuffer;
}
/**
 * @__sig ipppp
 */
function _napi_create_arraybuffer(env, byte_length, data, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('result') }}};
        var arrayBuffer = emnapiCreateArrayBuffer(byte_length, data, false);
        value = emnapiCtx.addToCurrentScope(arrayBuffer).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppp
 */
function _node_api_create_sharedarraybuffer(env, byte_length, data, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('result') }}};
        var arrayBuffer = emnapiCreateArrayBuffer(byte_length, data, true);
        value = emnapiCtx.addToCurrentScope(arrayBuffer).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipdp
 */
function _napi_create_date(env, time, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = emnapiCtx.addToCurrentScope(new Date(time)).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippppp
 */
function _napi_create_external(env, data, finalize_cb, finalize_hint, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!emnapiCtx.feature.supportFinalizer && finalize_cb) {
            throw emnapiCtx.createNotSupportWeakRefError('napi_create_external', 'Parameter "finalize_cb" must be 0(NULL)');
        }
        var externalHandle = emnapiCtx.getCurrentScope().addExternal(data);
        if (finalize_cb) {
            emnapiCtx.createReferenceWithFinalizer(envObject, externalHandle.id, 0, 0 /* ReferenceOwnership.kRuntime */, finalize_cb, data, finalize_hint);
        }
        {{{ from64('result') }}};
        value = externalHandle.id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppppp
 */
function _napi_create_external_arraybuffer(env, external_data, byte_length, finalize_cb, finalize_hint, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('byte_length') }}};
        {{{ from64('external_data') }}};
        {{{ from64('result') }}};
        byte_length = byte_length >>> 0;
        if (!external_data) {
            byte_length = 0;
        }
        if ((external_data + byte_length) > wasmMemory.buffer.byteLength) {
            throw new RangeError('Memory out of range');
        }
        if (!emnapiCtx.feature.supportFinalizer && finalize_cb) {
            throw emnapiCtx.createNotSupportWeakRefError('napi_create_external_arraybuffer', 'Parameter "finalize_cb" must be 0(NULL)');
        }
        var arrayBuffer = new ArrayBuffer(byte_length);
        if (byte_length === 0) {
            try {
                var MessageChannel_1 = emnapiCtx.feature.MessageChannel;
                var messageChannel = new MessageChannel_1();
                messageChannel.port1.postMessage(arrayBuffer, [arrayBuffer]);
            }
            catch (_) { }
        }
        else {
            var u8arr = new Uint8Array(arrayBuffer);
            u8arr.set(new Uint8Array(wasmMemory.buffer).subarray(external_data, external_data + byte_length));
            emnapiExternalMemory.table.set(arrayBuffer, {
                address: external_data,
                ownership: 1 /* ReferenceOwnership.kUserland */,
                runtimeAllocated: 0
            });
        }
        var handle = emnapiCtx.addToCurrentScope(arrayBuffer);
        if (finalize_cb) {
            var status_1 = _napi_add_finalizer(env, handle.id, external_data, finalize_cb, finalize_hint, /* NULL */ 0);
            if (status_1 === 10 /* napi_status.napi_pending_exception */) {
                var err = envObject.tryCatch.extractException();
                envObject.clearLastError();
                throw err;
            }
            else if (status_1 !== 0 /* napi_status.napi_ok */) {
                return envObject.setLastError(status_1);
            }
        }
        value = handle.id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipp
 */
function _napi_create_object(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope({}).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ipppppp
 */
function _node_api_create_object_with_properties(env, prototype_or_null, property_names, property_values, property_count, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('property_count') }}};
    property_count = property_count >>> 0;
    if (property_count > 0) {
        if (!property_names)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!property_values)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    var v8_prototype_or_null = prototype_or_null
        ? emnapiCtx.handleStore.get(prototype_or_null).value
        : null;
    var properties = {};
    {{{ from64('property_names') }}};
    {{{ from64('property_values') }}};
    for (var i = 0; i < property_count; i++) {
        var name_value = emnapiCtx.handleStore.get({{{ makeGetValue('property_names', 'i * ' + POINTER_SIZE, '*') }}}).value;
        if (!(typeof name_value === "string" || typeof name_value === "symbol"))
            return envObject.setLastError(4 /* napi_status.napi_name_expected */);
        properties[name_value] = {
            value: emnapiCtx.handleStore.get({{{ makeGetValue('property_values', 'i * ' + POINTER_SIZE, '*') }}}).value,
            writable: true,
            enumerable: true,
            configurable: true
        };
    }
    var obj;
    try {
        obj = Object.defineProperties(Object.create(v8_prototype_or_null), properties);
    }
    catch (_) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(obj).id;
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/**
 * @__sig ippp
 */
function _napi_create_symbol(env, description, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    if (!description) {
        // eslint-disable-next-line symbol-description, @typescript-eslint/no-unused-vars
        var value = emnapiCtx.addToCurrentScope(Symbol()).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
    }
    else {
        var handle = emnapiCtx.handleStore.get(description);
        var desc = handle.value;
        if (typeof desc !== 'string') {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var v = emnapiCtx.addToCurrentScope(Symbol(desc)).id;
        {{{ makeSetValue('result', 0, 'v', '*') }}};
    }
    return envObject.clearLastError();
}
/**
 * @__sig ipipppp
 */
function _napi_create_typedarray(env, type, length, arraybuffer, byte_offset, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(arraybuffer);
        if (!handle.isArrayBuffer()) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var buffer = handle.value;
        {{{ from64('byte_offset') }}};
        {{{ from64('length') }}};
        var createTypedArray = function (envObject, Type, size_of_element, buffer, byte_offset, length) {
            var _a;
            byte_offset = byte_offset >>> 0;
            length = length >>> 0;
            if (size_of_element > 1) {
                if ((byte_offset) % (size_of_element) !== 0) {
                    var err = new RangeError("start offset of ".concat((_a = Type.name) !== null && _a !== void 0 ? _a : '', " should be a multiple of ").concat(size_of_element));
                    err.code = 'ERR_NAPI_INVALID_TYPEDARRAY_ALIGNMENT';
                    envObject.tryCatch.setError(err);
                    return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
                }
            }
            if (((length * size_of_element) + byte_offset) > buffer.byteLength) {
                var err = new RangeError('Invalid typed array length');
                err.code = 'ERR_NAPI_INVALID_TYPEDARRAY_LENGTH';
                envObject.tryCatch.setError(err);
                return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
            }
            var out = new Type(buffer, byte_offset, length);
            if (buffer === wasmMemory.buffer) {
                if (!emnapiExternalMemory.wasmMemoryViewTable.has(out)) {
                    emnapiExternalMemory.wasmMemoryViewTable.set(out, {
                        Ctor: Type,
                        address: byte_offset,
                        length: length,
                        ownership: 1 /* ReferenceOwnership.kUserland */,
                        runtimeAllocated: 0
                    });
                }
            }
            {{{ from64('result') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            value = emnapiCtx.addToCurrentScope(out).id;
            {{{ makeSetValue('result', 0, 'value', '*') }}};
            return envObject.getReturnStatus();
        };
        switch (type) {
            case 0 /* napi_typedarray_type.napi_int8_array */:
                return createTypedArray(envObject, Int8Array, 1, buffer, byte_offset, length);
            case 1 /* napi_typedarray_type.napi_uint8_array */:
                return createTypedArray(envObject, Uint8Array, 1, buffer, byte_offset, length);
            case 2 /* napi_typedarray_type.napi_uint8_clamped_array */:
                return createTypedArray(envObject, Uint8ClampedArray, 1, buffer, byte_offset, length);
            case 3 /* napi_typedarray_type.napi_int16_array */:
                return createTypedArray(envObject, Int16Array, 2, buffer, byte_offset, length);
            case 4 /* napi_typedarray_type.napi_uint16_array */:
                return createTypedArray(envObject, Uint16Array, 2, buffer, byte_offset, length);
            case 5 /* napi_typedarray_type.napi_int32_array */:
                return createTypedArray(envObject, Int32Array, 4, buffer, byte_offset, length);
            case 6 /* napi_typedarray_type.napi_uint32_array */:
                return createTypedArray(envObject, Uint32Array, 4, buffer, byte_offset, length);
            case 7 /* napi_typedarray_type.napi_float32_array */:
                return createTypedArray(envObject, Float32Array, 4, buffer, byte_offset, length);
            case 8 /* napi_typedarray_type.napi_float64_array */:
                return createTypedArray(envObject, Float64Array, 8, buffer, byte_offset, length);
            case 9 /* napi_typedarray_type.napi_bigint64_array */:
                return createTypedArray(envObject, BigInt64Array, 8, buffer, byte_offset, length);
            case 10 /* napi_typedarray_type.napi_biguint64_array */:
                return createTypedArray(envObject, BigUint64Array, 8, buffer, byte_offset, length);
            case 11 /* napi_typedarray_type.napi_float16_array */:
                if (typeof Float16Array !== 'function') {
                    return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
                }
                return createTypedArray(envObject, Float16Array, 2, buffer, byte_offset, length);
            default:
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__deps malloc
 * @__sig ippp
 */
function _napi_create_buffer(env, size, data, result) {
    var _a;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value, pointer;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var Buffer = emnapiCtx.feature.Buffer;
        if (!Buffer) {
            throw emnapiCtx.createNotSupportBufferError('napi_create_buffer', '');
        }
        {{{ from64('result') }}};
        var buffer = void 0;
        {{{ from64('size') }}};
        size = size >>> 0;
        if (!data || (size === 0)) {
            buffer = Buffer.alloc(size);
            value = emnapiCtx.addToCurrentScope(buffer).id;
            {{{ makeSetValue('result', 0, 'value', '*') }}};
        }
        else {
            pointer = _malloc({{{ to64('size') }}});
            if (!pointer)
                throw new Error('Out of memory');
            new Uint8Array(wasmMemory.buffer).subarray(pointer, pointer + size).fill(0);
            var buffer_1 = Buffer.from(wasmMemory.buffer, pointer, size);
            var viewDescriptor = {
                Ctor: Buffer,
                address: pointer,
                length: size,
                ownership: emnapiExternalMemory.registry ? 0 /* ReferenceOwnership.kRuntime */ : 1 /* ReferenceOwnership.kUserland */,
                runtimeAllocated: 1
            };
            emnapiExternalMemory.wasmMemoryViewTable.set(buffer_1, viewDescriptor);
            (_a = emnapiExternalMemory.registry) === null || _a === void 0 ? void 0 : _a.register(viewDescriptor, pointer);
            value = emnapiCtx.addToCurrentScope(buffer_1).id;
            {{{ makeSetValue('result', 0, 'value', '*') }}};
            {{{ from64('data') }}};
            {{{ makeSetValue('data', 0, 'pointer', '*') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippppp
 */
function _napi_create_buffer_copy(env, length, data, result_data, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var Buffer = emnapiCtx.feature.Buffer;
        if (!Buffer) {
            throw emnapiCtx.createNotSupportBufferError('napi_create_buffer_copy', '');
        }
        var arrayBuffer = emnapiCreateArrayBuffer(length, result_data, false);
        var buffer = Buffer.from(arrayBuffer);
        {{{ from64('data') }}};
        {{{ from64('length') }}};
        buffer.set(new Uint8Array(wasmMemory.buffer).subarray(data, data + length));
        value = emnapiCtx.addToCurrentScope(buffer).id;
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppppp
 */
function _napi_create_external_buffer(env, length, data, finalize_cb, finalize_hint, result) {
    return _emnapi_create_memory_view(env, -2 /* emnapi_memory_view_type.emnapi_buffer */, data, length, finalize_cb, finalize_hint, result);
}
/**
 * @__sig ippppp
 */
function _node_api_create_buffer_from_arraybuffer(env, arraybuffer, byte_offset, byte_length, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('byte_offset') }}};
        {{{ from64('byte_length') }}};
        byte_offset = byte_offset >>> 0;
        byte_length = byte_length >>> 0;
        var handle = emnapiCtx.handleStore.get(arraybuffer);
        if (!handle.isArrayBuffer()) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var buffer = handle.value;
        if ((byte_length + byte_offset) > buffer.byteLength) {
            var err = new RangeError('The byte offset + length is out of range');
            err.code = 'ERR_OUT_OF_RANGE';
            throw err;
        }
        var Buffer = emnapiCtx.feature.Buffer;
        if (!Buffer) {
            throw emnapiCtx.createNotSupportBufferError('node_api_create_buffer_from_arraybuffer', '');
        }
        var out = Buffer.from(buffer, byte_offset, byte_length);
        if (buffer === wasmMemory.buffer) {
            if (!emnapiExternalMemory.wasmMemoryViewTable.has(out)) {
                emnapiExternalMemory.wasmMemoryViewTable.set(out, {
                    Ctor: Buffer,
                    address: byte_offset,
                    length: byte_length,
                    ownership: 1 /* ReferenceOwnership.kUserland */,
                    runtimeAllocated: 0
                });
            }
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = emnapiCtx.addToCurrentScope(out).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ippppp
 */
function _napi_create_dataview(env, byte_length, arraybuffer, byte_offset, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('byte_length') }}};
        {{{ from64('byte_offset') }}};
        byte_length = byte_length >>> 0;
        byte_offset = byte_offset >>> 0;
        var value = emnapiCtx.handleStore.get(arraybuffer).value;
        var createDataview = function (buffer) {
            if ((byte_length + byte_offset) > buffer.byteLength) {
                var err = new RangeError('byte_offset + byte_length should be less than or equal to the size in bytes of the array passed in');
                err.code = 'ERR_NAPI_INVALID_DATAVIEW_ARGS';
                throw err;
            }
            var dataview = new DataView(buffer, byte_offset, byte_length);
            if (buffer === wasmMemory.buffer) {
                if (!emnapiExternalMemory.wasmMemoryViewTable.has(dataview)) {
                    emnapiExternalMemory.wasmMemoryViewTable.set(dataview, {
                        Ctor: DataView,
                        address: byte_offset,
                        length: byte_length,
                        ownership: 1 /* ReferenceOwnership.kUserland */,
                        runtimeAllocated: 0
                    });
                }
            }
            {{{ from64('result') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var v = emnapiCtx.addToCurrentScope(dataview).id;
            {{{ makeSetValue('result', 0, 'v', '*') }}};
            return envObject.getReturnStatus();
        };
        if (value instanceof ArrayBuffer || emnapiExternalMemory.isSharedArrayBuffer(value)) {
            return createDataview(value);
        }
        else {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/**
 * @__sig ipppp
 */
function _node_api_symbol_for(env, utf8description, length, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('length') }}};
    {{{ from64('utf8description') }}};
    {{{ from64('result') }}};
    var autoLength = length === -1;
    var sizelength = length >>> 0;
    if (length !== 0) {
        if (!utf8description)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    if (!(autoLength || (sizelength <= 2147483647))) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    var descriptionString = emnapiString.UTF8ToString(utf8description, length);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(Symbol.for(descriptionString)).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
// eslint-disable-next-line @typescript-eslint/no-unused-vars
/** @__sig ipip */
function _napi_get_boolean(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v = value === 0 ? 3 /* GlobalHandle.FALSE */ : 4 /* GlobalHandle.TRUE */;
    {{{ makeSetValue('result', 0, 'v', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_global(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = 5 /* GlobalHandle.GLOBAL */;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_null(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = 2 /* GlobalHandle.NULL */;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_undefined(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = 1 /* GlobalHandle.UNDEFINED */;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_set_instance_data(env, data, finalize_cb, finalize_hint) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    {{{ from64('data') }}};
    {{{ from64('finalize_cb') }}};
    {{{ from64('finalize_hint') }}};
    envObject.setInstanceData(data, finalize_cb, finalize_hint);
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_instance_data(env, data) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!data)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('data') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = envObject.getInstanceData();
    {{{ makeSetValue('data', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig vpppp */
function __emnapi_get_last_error_info(env, error_code, engine_error_code, engine_reserved) {
    {{{ from64('error_code') }}};
    {{{ from64('engine_error_code') }}};
    {{{ from64('engine_reserved') }}};
    var envObject = emnapiCtx.envStore.get(env);
    var lastError = envObject.lastError;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var errorCode = lastError.errorCode;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var engineErrorCode = lastError.engineErrorCode >>> 0;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var engineReserved = lastError.engineReserved;
    {{{ from64('engineReserved') }}};
    {{{ makeSetValue('error_code', 0, 'errorCode', 'i32') }}};
    {{{ makeSetValue('engine_error_code', 0, 'engineErrorCode', 'u32') }}};
    {{{ makeSetValue('engine_reserved', 0, 'engineReserved', '*') }}};
}
/** @__sig ipp */
function _napi_throw(env, error) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!error)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        envObject.tryCatch.setError(emnapiCtx.handleStore.get(error).value);
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_throw_error(env, code, msg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!msg)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('code') }}};
        {{{ from64('msg') }}};
        var error = new Error(emnapiString.UTF8ToString(msg, -1));
        if (code)
            error.code = emnapiString.UTF8ToString(code, -1);
        envObject.tryCatch.setError(error);
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_throw_type_error(env, code, msg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!msg)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('code') }}};
        {{{ from64('msg') }}};
        var error = new TypeError(emnapiString.UTF8ToString(msg, -1));
        if (code)
            error.code = emnapiString.UTF8ToString(code, -1);
        envObject.tryCatch.setError(error);
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_throw_range_error(env, code, msg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!msg)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('code') }}};
        {{{ from64('msg') }}};
        var error = new RangeError(emnapiString.UTF8ToString(msg, -1));
        if (code)
            error.code = emnapiString.UTF8ToString(code, -1);
        envObject.tryCatch.setError(error);
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _node_api_throw_syntax_error(env, code, msg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!msg)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('code') }}};
        {{{ from64('msg') }}};
        var error = new SyntaxError(emnapiString.UTF8ToString(msg, -1));
        if (code)
            error.code = emnapiString.UTF8ToString(code, -1);
        envObject.tryCatch.setError(error);
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipp */
function _napi_is_exception_pending(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = envObject.tryCatch.hasCaught();
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'r ? 1 : 0', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_create_error(env, code, msg, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!msg)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var msgValue = emnapiCtx.handleStore.get(msg).value;
    if (typeof msgValue !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    var error = new Error(msgValue);
    if (code) {
        var codeValue = emnapiCtx.handleStore.get(code).value;
        if (typeof codeValue !== 'string') {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        error.code = codeValue;
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(error).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_create_type_error(env, code, msg, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!msg)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var msgValue = emnapiCtx.handleStore.get(msg).value;
    if (typeof msgValue !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    var error = new TypeError(msgValue);
    if (code) {
        var codeValue = emnapiCtx.handleStore.get(code).value;
        if (typeof codeValue !== 'string') {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        error.code = codeValue;
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(error).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_create_range_error(env, code, msg, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!msg)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var msgValue = emnapiCtx.handleStore.get(msg).value;
    if (typeof msgValue !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    var error = new RangeError(msgValue);
    if (code) {
        var codeValue = emnapiCtx.handleStore.get(code).value;
        if (typeof codeValue !== 'string') {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        error.code = codeValue;
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(error).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _node_api_create_syntax_error(env, code, msg, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!msg)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var msgValue = emnapiCtx.handleStore.get(msg).value;
    if (typeof msgValue !== 'string') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    var error = new SyntaxError(msgValue);
    if (code) {
        var codeValue = emnapiCtx.handleStore.get(code).value;
        if (typeof codeValue !== 'string') {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        error.code = codeValue;
    }
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = emnapiCtx.addToCurrentScope(error).id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_and_clear_last_exception(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    if (!envObject.tryCatch.hasCaught()) {
        {{{ makeSetValue('result', 0, '1', '*') }}}; // ID_UNDEFINED
        return envObject.clearLastError();
    }
    else {
        var err = envObject.tryCatch.exception();
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var value = envObject.ensureHandleId(err);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        envObject.tryCatch.reset();
    }
    return envObject.clearLastError();
}
/** @__sig vpppp */
function _napi_fatal_error(location, location_len, message, message_len) {
    {{{ from64('location') }}};
    {{{ from64('location_len') }}};
    {{{ from64('message') }}};
    {{{ from64('message_len') }}};
    var locationStr = emnapiString.UTF8ToString(location, location_len);
    var messageStr = emnapiString.UTF8ToString(message, message_len);
    if (emnapiNodeBinding) {
        emnapiNodeBinding.napi.fatalError(locationStr, messageStr);
    }
    else {
        abort('FATAL ERROR: ' + locationStr + ' ' + messageStr);
    }
}
/** @__sig ipp */
function _napi_fatal_exception(env, err) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!err)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var error = envObject.ctx.handleStore.get(err);
        try {
            envObject.triggerFatalException(error.value);
        }
        catch (_) {
            return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
        }
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppppp */
function __emnapi_create_function(env, utf8name, length, cb, data, result) {
    {{{ from64('length') }}};
    var envObject = emnapiCtx.envStore.get(env);
    var fresult = emnapiCreateFunction(envObject, utf8name, length, cb, data);
    if (fresult.status !== 0 /* napi_status.napi_ok */)
        return envObject.setLastError(fresult.status);
    var f = fresult.f;
    var valueHandle = emnapiCtx.addToCurrentScope(f);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value = valueHandle.id;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.getReturnStatus();
}
/** @__sig ipppppp */
function _napi_create_function(env, utf8name, length, cb, data, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!cb)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        return __emnapi_create_function(env, utf8name, length, cb, data, result);
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppppp */
function _napi_get_cb_info(env, cbinfo, argc, argv, this_arg, data) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!cbinfo)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var cbinfoValue = emnapiCtx.scopeStore.get(cbinfo).callbackInfo;
    {{{ from64('argc') }}};
    {{{ from64('argv') }}};
    if (argv) {
        if (!argc)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var argcValue = {{{ makeGetValue('argc', 0, SIZE_TYPE) }}};
        {{{ from64('argcValue') }}};
        var len = cbinfoValue.args.length;
        var arrlen = argcValue < len ? argcValue : len;
        var i = 0;
        for (; i < arrlen; i++) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            var argVal = envObject.ensureHandleId(cbinfoValue.args[i]);
            {{{ makeSetValue('argv', 'i * ' + POINTER_SIZE, 'argVal', '*') }}};
        }
        if (i < argcValue) {
            for (; i < argcValue; i++) {
                {{{ makeSetValue('argv', 'i * ' + POINTER_SIZE, '1', '*') }}};
            }
        }
    }
    if (argc) {
        {{{ makeSetValue('argc', 0, 'cbinfoValue.args.length', SIZE_TYPE) }}};
    }
    if (this_arg) {
        {{{ from64('this_arg') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var v = envObject.ensureHandleId(cbinfoValue.thiz);
        {{{ makeSetValue('this_arg', 0, 'v', '*') }}};
    }
    if (data) {
        {{{ from64('data') }}};
        {{{ makeSetValue('data', 0, 'cbinfoValue.data', '*') }}};
    }
    return envObject.clearLastError();
}
/** @__sig ipppppp */
function _napi_call_function(env, recv, func, argc, argv, result) {
    var i = 0;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!recv)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('argc') }}};
        {{{ from64('argv') }}};
        {{{ from64('result') }}};
        argc = argc >>> 0;
        if (argc > 0) {
            if (!argv)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var v8recv = emnapiCtx.handleStore.get(recv).value;
        if (!func)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var v8func = emnapiCtx.handleStore.get(func).value;
        if (typeof v8func !== 'function')
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var args = [];
        for (; i < argc; i++) {
            var argVal = {{{ makeGetValue('argv', 'i * ' + POINTER_SIZE, '*') }}};
            args.push(emnapiCtx.handleStore.get(argVal).value);
        }
        var ret = v8func.apply(v8recv, args);
        if (result) {
            v = envObject.ensureHandleId(ret);
            {{{ makeSetValue('result', 0, 'v', '*') }}};
        }
        return envObject.clearLastError();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippppp */
function _napi_new_instance(env, constructor, argc, argv, result) {
    var i;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!constructor)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('argc') }}};
        {{{ from64('argv') }}};
        {{{ from64('result') }}};
        argc = argc >>> 0;
        if (argc > 0) {
            if (!argv)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var Ctor = emnapiCtx.handleStore.get(constructor).value;
        if (typeof Ctor !== 'function')
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var ret = void 0;
        if (emnapiCtx.feature.supportReflect) {
            var argList = Array(argc);
            for (i = 0; i < argc; i++) {
                var argVal = {{{ makeGetValue('argv', 'i * ' + POINTER_SIZE, '*') }}};
                argList[i] = emnapiCtx.handleStore.get(argVal).value;
            }
            ret = Reflect.construct(Ctor, argList, Ctor);
        }
        else {
            var args = Array(argc + 1);
            args[0] = undefined;
            for (i = 0; i < argc; i++) {
                var argVal = {{{ makeGetValue('argv', 'i * ' + POINTER_SIZE, '*') }}};
                args[i + 1] = emnapiCtx.handleStore.get(argVal).value;
            }
            var BoundCtor = Ctor.bind.apply(Ctor, args);
            ret = new BoundCtor();
        }
        if (result) {
            v = envObject.ensureHandleId(ret);
            {{{ makeSetValue('result', 0, 'v', '*') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_get_new_target(env, cbinfo, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!cbinfo)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('result') }}};
    var cbinfoValue = emnapiCtx.scopeStore.get(cbinfo).callbackInfo;
    var thiz = cbinfoValue.thiz, fn = cbinfoValue.fn;
    // eslint-disable-next-line @typescript-eslint/prefer-optional-chain, @typescript-eslint/no-unused-vars
    var value = thiz == null || thiz.constructor == null
        ? 0
        : thiz instanceof fn
            ? envObject.ensureHandleId(thiz.constructor)
            : 0;
    {{{ makeSetValue('result', 0, 'value', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_open_handle_scope(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var scope = emnapiCtx.openScope(envObject);
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'scope.id', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_close_handle_scope(env, scope) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!scope)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if ((envObject.openHandleScopes === 0)) {
        return 13 /* napi_status.napi_handle_scope_mismatch */;
    }
    emnapiCtx.closeScope(envObject);
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_open_escapable_handle_scope(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var scope = emnapiCtx.openScope(envObject);
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'scope.id', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_close_escapable_handle_scope(env, scope) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!scope)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if ((envObject.openHandleScopes === 0)) {
        return 13 /* napi_status.napi_handle_scope_mismatch */;
    }
    emnapiCtx.closeScope(envObject);
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_escape_handle(env, scope, escapee, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!scope)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!escapee)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var scopeObject = emnapiCtx.scopeStore.get(scope);
    if (!scopeObject.escapeCalled()) {
        {{{ from64('escapee') }}};
        {{{ from64('result') }}};
        var newHandle = scopeObject.escape(escapee);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var value = newHandle ? newHandle.id : 0;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.clearLastError();
    }
    return envObject.setLastError(12 /* napi_status.napi_escape_called_twice */);
}
/** @__sig ippip */
function _napi_create_reference(env, value, initial_refcount, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var handle = emnapiCtx.handleStore.get(value);
    if (envObject.moduleApiVersion < 10) {
        if (!(handle.isObject() || handle.isFunction() || handle.isSymbol())) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var ref = emnapiCtx.createReference(envObject, handle.id, initial_refcount >>> 0, 1 /* ReferenceOwnership.kUserland */);
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'ref.id', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_delete_reference(env, ref) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!ref)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    emnapiCtx.refStore.get(ref).dispose();
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_reference_ref(env, ref, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!ref)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var count = emnapiCtx.refStore.get(ref).ref();
    if (result) {
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'count', 'u32') }}};
    }
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_reference_unref(env, ref, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!ref)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var reference = emnapiCtx.refStore.get(ref);
    var refcount = reference.refcount();
    if (refcount === 0) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var count = reference.unref();
    if (result) {
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'count', 'u32') }}};
    }
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_get_reference_value(env, ref, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!ref)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var reference = emnapiCtx.refStore.get(ref);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var handleId = reference.get(envObject);
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'handleId', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_add_env_cleanup_hook(env, fun, arg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!fun)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('fun') }}};
    {{{ from64('arg') }}};
    emnapiCtx.addCleanupHook(envObject, fun, arg);
    return 0 /* napi_status.napi_ok */;
}
/** @__sig ippp */
function _napi_remove_env_cleanup_hook(env, fun, arg) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!fun)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('fun') }}};
    {{{ from64('arg') }}};
    emnapiCtx.removeCleanupHook(envObject, fun, arg);
    return 0 /* napi_status.napi_ok */;
}
/** @__sig vp */
function __emnapi_env_ref(env) {
    var envObject = emnapiCtx.envStore.get(env);
    envObject.ref();
}
/** @__sig vp */
function __emnapi_env_unref(env) {
    var envObject = emnapiCtx.envStore.get(env);
    envObject.unref();
}
/** @__sig ippi */
function __emnapi_get_filename(env, buf, len) {
    var envObject = emnapiCtx.envStore.get(env);
    var filename = envObject.filename;
    if (!buf) {
        return emnapiString.lengthBytesUTF8(filename);
    }
    return emnapiString.stringToUTF8(filename, buf, len);
}
/** @__sig vppdp */
function __emnapi_node_emit_async_init(async_resource, async_resource_name, trigger_async_id, result) {
    if (!emnapiNodeBinding)
        return;
    var resource = emnapiCtx.handleStore.get(async_resource).value;
    var resource_name = emnapiCtx.handleStore.get(async_resource_name).value;
    var asyncContext = emnapiNodeBinding.node.emitAsyncInit(resource, resource_name, trigger_async_id);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var asyncId = asyncContext.asyncId;
    var triggerAsyncId = asyncContext.triggerAsyncId;
    if (result) {
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'asyncId', 'double') }}};
        {{{ makeSetValue('result', 8, 'triggerAsyncId', 'double') }}};
    }
}
/** @__sig vdd */
function __emnapi_node_emit_async_destroy(async_id, trigger_async_id) {
    if (!emnapiNodeBinding)
        return;
    emnapiNodeBinding.node.emitAsyncDestroy({
        asyncId: async_id,
        triggerAsyncId: trigger_async_id
    });
}
/* vpddp export function _emnapi_node_open_callback_scope (async_resource: napi_value, async_id: double, trigger_async_id: double, result: Pointer<int64_t>): void {
  if (!emnapiNodeBinding || !result) return
  const resource = emnapiCtx.handleStore.get(async_resource)!.value
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const nativeCallbackScopePointer = emnapiNodeBinding.node.openCallbackScope(resource, {
    asyncId: async_id,
    triggerAsyncId: trigger_async_id
  })

  from64('result')
  $_TODO_makeSetValue('result', 0, 'nativeCallbackScopePointer', 'i64')
}

vp
export function _emnapi_node_close_callback_scope (scope: Pointer<int64_t>): void {
  if (!emnapiNodeBinding || !scope) return
  from64('scope')
  const nativeCallbackScopePointer = $_TODO_makeGetValue('scope', 0, 'i64')
  emnapiNodeBinding.node.closeCallbackScope(BigInt(nativeCallbackScopePointer))
} */
/** @__sig ipppppddp */
function __emnapi_node_make_callback(env, async_resource, cb, argv, size, async_id, trigger_async_id, result) {
    var i = 0;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!emnapiNodeBinding)
        return;
    var resource = emnapiCtx.handleStore.get(async_resource).value;
    var callback = emnapiCtx.handleStore.get(cb).value;
    {{{ from64('argv') }}};
    {{{ from64('size') }}};
    size = size >>> 0;
    var arr = Array(size);
    for (; i < size; i++) {
        var argVal = {{{ makeGetValue('argv', 'i * ' + POINTER_SIZE, '*') }}};
        arr[i] = emnapiCtx.handleStore.get(argVal).value;
    }
    var ret = emnapiNodeBinding.node.makeCallback(resource, callback, arr, {
        asyncId: async_id,
        triggerAsyncId: trigger_async_id
    });
    if (result) {
        {{{ from64('result') }}};
        var envObject = emnapiCtx.envStore.get(env);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        v = envObject.ensureHandleId(ret);
        {{{ makeSetValue('result', 0, 'v', '*') }}};
    }
}
/** @__sig ippp */
function __emnapi_async_init_js(async_resource, async_resource_name, result) {
    if (!emnapiNodeBinding) {
        return 9 /* napi_status.napi_generic_failure */;
    }
    var resource;
    if (async_resource) {
        resource = Object(emnapiCtx.handleStore.get(async_resource).value);
    }
    var name = emnapiCtx.handleStore.get(async_resource_name).value;
    var ret = emnapiNodeBinding.napi.asyncInit(resource, name);
    if (ret.status !== 0)
        return ret.status;
    var numberValue = ret.value;
    if (!((numberValue >= (BigInt(-1) * (BigInt(1) << BigInt(63)))) && (numberValue < (BigInt(1) << BigInt(63))))) {
        numberValue = numberValue & ((BigInt(1) << BigInt(64)) - BigInt(1));
        if (numberValue >= (BigInt(1) << BigInt(63))) {
            numberValue = numberValue - (BigInt(1) << BigInt(64));
        }
    }
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var low = Number(numberValue & BigInt(0xffffffff));
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var high = Number(numberValue >> BigInt(32));
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'low', 'i32') }}};
    {{{ makeSetValue('result', 4, 'high', 'i32') }}};
    return 0 /* napi_status.napi_ok */;
}
/** @__sig ip */
function __emnapi_async_destroy_js(async_context) {
    if (!emnapiNodeBinding) {
        return 9 /* napi_status.napi_generic_failure */;
    }
    {{{ from64('async_context') }}};
    var low = {{{ makeGetValue('async_context', 0, 'i32') }}};
    var high = {{{ makeGetValue('async_context', 4, 'i32') }}};
    var pointer = BigInt(low >>> 0) | (BigInt(high) << BigInt(32));
    var ret = emnapiNodeBinding.napi.asyncDestroy(pointer);
    if (ret.status !== 0)
        return ret.status;
    return 0 /* napi_status.napi_ok */;
}
// https://github.com/nodejs/node-addon-api/pull/1283
/** @__sig ipppp */
function _napi_open_callback_scope(env, ignored, async_context_handle, result) {
    throw new Error('napi_open_callback_scope has not been implemented yet');
}
/** @__sig ipp */
function _napi_close_callback_scope(env, scope) {
    throw new Error('napi_close_callback_scope has not been implemented yet');
}
/** @__sig ippppppp */
function _napi_make_callback(env, async_context, recv, func, argc, argv, result) {
    var i = 0;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!emnapiNodeBinding) {
            return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
        }
        if (!recv)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (argc > 0) {
            if (!argv)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var v8recv = Object(emnapiCtx.handleStore.get(recv).value);
        var v8func = emnapiCtx.handleStore.get(func).value;
        if (typeof v8func !== 'function') {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        {{{ from64('async_context') }}};
        var low = {{{ makeGetValue('async_context', 0, 'i32') }}};
        var high = {{{ makeGetValue('async_context', 4, 'i32') }}};
        var ctx = BigInt(low >>> 0) | (BigInt(high) << BigInt(32));
        {{{ from64('argv') }}};
        {{{ from64('argc') }}};
        argc = argc >>> 0;
        var arr = Array(argc);
        for (; i < argc; i++) {
            var argVal = {{{ makeGetValue('argv', 'i * ' + POINTER_SIZE, '*') }}};
            arr[i] = emnapiCtx.handleStore.get(argVal).value;
        }
        var ret = emnapiNodeBinding.napi.makeCallback(ctx, v8recv, v8func, arr);
        if (ret.error) {
            throw ret.error;
        }
        if (ret.status !== 0 /* napi_status.napi_ok */)
            return envObject.setLastError(ret.status);
        if (result) {
            {{{ from64('result') }}};
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            v = envObject.ensureHandleId(ret.value);
            {{{ makeSetValue('result', 0, 'v', '*') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig vp */
function __emnapi_env_check_gc_access(env) {
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
}
/** @__sig ippp */
function _napi_create_promise(env, deferred, promise) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var deferredObjectId, value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!deferred)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!promise)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var p = new Promise(function (resolve, reject) {
            var deferredObject = emnapiCtx.createDeferred({ resolve: resolve, reject: reject });
            deferredObjectId = deferredObject.id;
            {{{ from64('deferred') }}};
            {{{ makeSetValue('deferred', 0, 'deferredObjectId', '*') }}};
        });
        {{{ from64('promise') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = emnapiCtx.addToCurrentScope(p).id;
        {{{ makeSetValue('promise', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_resolve_deferred(env, deferred, resolution) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!deferred)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!resolution)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var deferredObject = emnapiCtx.deferredStore.get(deferred);
        deferredObject.resolve(emnapiCtx.handleStore.get(resolution).value);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_reject_deferred(env, deferred, resolution) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!deferred)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!resolution)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var deferredObject = emnapiCtx.deferredStore.get(deferred);
        deferredObject.reject(emnapiCtx.handleStore.get(resolution).value);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_is_promise(env, value, is_promise) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!is_promise)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('is_promise') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isPromise() ? 1 : 0;
    {{{ makeSetValue('is_promise', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippiiip */
function _napi_get_all_property_names(env, object, key_mode, key_filter, key_conversion, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var obj = void 0;
        try {
            obj = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        if (key_mode !== 0 /* napi_key_collection_mode.napi_key_include_prototypes */ && key_mode !== 1 /* napi_key_collection_mode.napi_key_own_only */) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (key_conversion !== 0 /* napi_key_conversion.napi_key_keep_numbers */ && key_conversion !== 1 /* napi_key_conversion.napi_key_numbers_to_strings */) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var props = [];
        var names = void 0;
        var symbols = void 0;
        var i = void 0;
        var own = true;
        var integerIndiceRegex = /^(0|[1-9][0-9]*)$/;
        do {
            names = Object.getOwnPropertyNames(obj);
            symbols = Object.getOwnPropertySymbols(obj);
            for (i = 0; i < names.length; i++) {
                props.push({
                    name: integerIndiceRegex.test(names[i]) ? Number(names[i]) : names[i],
                    desc: Object.getOwnPropertyDescriptor(obj, names[i]),
                    own: own
                });
            }
            for (i = 0; i < symbols.length; i++) {
                props.push({
                    name: symbols[i],
                    desc: Object.getOwnPropertyDescriptor(obj, symbols[i]),
                    own: own
                });
            }
            if (key_mode === 1 /* napi_key_collection_mode.napi_key_own_only */) {
                break;
            }
            obj = Object.getPrototypeOf(obj);
            own = false;
        } while (obj);
        var ret = [];
        var addName = function (ret, name, key_filter, conversion_mode) {
            if (ret.indexOf(name) !== -1)
                return;
            if (conversion_mode === 0 /* napi_key_conversion.napi_key_keep_numbers */) {
                ret.push(name);
            }
            else if (conversion_mode === 1 /* napi_key_conversion.napi_key_numbers_to_strings */) {
                var realName = typeof name === 'number' ? String(name) : name;
                if (typeof realName === 'string') {
                    if (!(key_filter & 8 /* napi_key_filter.napi_key_skip_strings */)) {
                        ret.push(realName);
                    }
                }
                else {
                    ret.push(realName);
                }
            }
        };
        for (i = 0; i < props.length; i++) {
            var prop = props[i];
            var name_1 = prop.name;
            var desc = prop.desc;
            if (key_filter === 0 /* napi_key_filter.napi_key_all_properties */) {
                addName(ret, name_1, key_filter, key_conversion);
            }
            else {
                if (key_filter & 8 /* napi_key_filter.napi_key_skip_strings */ && typeof name_1 === 'string') {
                    continue;
                }
                if (key_filter & 16 /* napi_key_filter.napi_key_skip_symbols */ && typeof name_1 === 'symbol') {
                    continue;
                }
                var shouldAdd = true;
                switch (key_filter & 7) {
                    case 1 /* napi_key_filter.napi_key_writable */: {
                        shouldAdd = Boolean(desc.writable);
                        break;
                    }
                    case 2 /* napi_key_filter.napi_key_enumerable */: {
                        shouldAdd = Boolean(desc.enumerable);
                        break;
                    }
                    case (1 /* napi_key_filter.napi_key_writable */ | 2 /* napi_key_filter.napi_key_enumerable */): {
                        shouldAdd = Boolean(desc.writable && desc.enumerable);
                        break;
                    }
                    case 4 /* napi_key_filter.napi_key_configurable */: {
                        shouldAdd = Boolean(desc.configurable);
                        break;
                    }
                    case (4 /* napi_key_filter.napi_key_configurable */ | 1 /* napi_key_filter.napi_key_writable */): {
                        shouldAdd = Boolean(desc.configurable && desc.writable);
                        break;
                    }
                    case (4 /* napi_key_filter.napi_key_configurable */ | 2 /* napi_key_filter.napi_key_enumerable */): {
                        shouldAdd = Boolean(desc.configurable && desc.enumerable);
                        break;
                    }
                    case (4 /* napi_key_filter.napi_key_configurable */ | 2 /* napi_key_filter.napi_key_enumerable */ | 1 /* napi_key_filter.napi_key_writable */): {
                        shouldAdd = Boolean(desc.configurable && desc.enumerable && desc.writable);
                        break;
                    }
                }
                if (shouldAdd) {
                    addName(ret, name_1, key_filter, key_conversion);
                }
            }
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = emnapiCtx.addToCurrentScope(ret).id;
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_get_property_names(env, object, result) {
    return _napi_get_all_property_names(env, object, 0 /* napi_key_collection_mode.napi_key_include_prototypes */, 2 /* napi_key_filter.napi_key_enumerable */ | 16 /* napi_key_filter.napi_key_skip_symbols */, 1 /* napi_key_conversion.napi_key_numbers_to_strings */, result);
}
/** @__sig ipppp */
function _napi_set_property(env, object, key, value) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!key)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        h.value[emnapiCtx.handleStore.get(key).value] = emnapiCtx.handleStore.get(value).value;
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_has_property(env, object, key, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!key)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('result') }}};
        r = (emnapiCtx.handleStore.get(key).value in v) ? 1 : 0;
        {{{ makeSetValue('result', 0, 'r', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_get_property(env, object, key, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!key)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(v[emnapiCtx.handleStore.get(key).value]);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_delete_property(env, object, key, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!key)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        var propertyKey = emnapiCtx.handleStore.get(key).value;
        if (emnapiCtx.feature.supportReflect) {
            r = Reflect.deleteProperty(h.value, propertyKey);
        }
        else {
            try {
                r = delete h.value[propertyKey];
            }
            catch (_) {
                r = false;
            }
        }
        if (result) {
            {{{ from64('result') }}};
            {{{ makeSetValue('result', 0, 'r ? 1 : 0', 'i8') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_has_own_property(env, object, key, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value, r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!key)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        var prop = emnapiCtx.handleStore.get(key).value;
        if (typeof prop !== 'string' && typeof prop !== 'symbol') {
            return envObject.setLastError(4 /* napi_status.napi_name_expected */);
        }
        r = Object.prototype.hasOwnProperty.call(v, emnapiCtx.handleStore.get(key).value);
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, 'r ? 1 : 0', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_set_named_property(env, object, cname, value) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        if (!cname) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        {{{ from64('cname') }}};
        emnapiCtx.handleStore.get(object).value[emnapiString.UTF8ToString(cname, -1)] = emnapiCtx.handleStore.get(value).value;
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_has_named_property(env, object, utf8name, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!utf8name) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('utf8name') }}};
        {{{ from64('result') }}};
        r = emnapiString.UTF8ToString(utf8name, -1) in v;
        {{{ makeSetValue('result', 0, 'r ? 1 : 0', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_get_named_property(env, object, utf8name, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!utf8name) {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('utf8name') }}};
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(v[emnapiString.UTF8ToString(utf8name, -1)]);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippip */
function _napi_set_element(env, object, index, value) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        h.value[index >>> 0] = emnapiCtx.handleStore.get(value).value;
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippip */
function _napi_has_element(env, object, index, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('result') }}};
        r = ((index >>> 0) in v) ? 1 : 0;
        {{{ makeSetValue('result', 0, 'r', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippip */
function _napi_get_element(env, object, index, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (h.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        var v = void 0;
        try {
            v = h.isObject() || h.isFunction() ? h.value : Object(h.value);
        }
        catch (_) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(v[index >>> 0]);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippip */
function _napi_delete_element(env, object, index, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        if (emnapiCtx.feature.supportReflect) {
            r = Reflect.deleteProperty(h.value, index >>> 0);
        }
        else {
            try {
                r = delete h.value[index >>> 0];
            }
            catch (_) {
                r = false;
            }
        }
        if (result) {
            {{{ from64('result') }}};
            {{{ makeSetValue('result', 0, 'r ? 1 : 0', 'i8') }}};
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_define_properties(env, object, property_count, properties) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var propPtr, attributes;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        {{{ from64('properties') }}};
        {{{ from64('property_count') }}};
        property_count = property_count >>> 0;
        if (property_count > 0) {
            if (!properties)
                return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        var maybeObject = h.value;
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        var propertyName = void 0;
        for (var i = 0; i < property_count; i++) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            propPtr = properties + (i * ({{{ POINTER_SIZE }}} * 8));
            var utf8Name = {{{ makeGetValue('propPtr', 0, '*') }}};
            var name_2 = {{{ makeGetValue('propPtr', POINTER_SIZE, '*') }}};
            var method = {{{ makeGetValue('propPtr', POINTER_SIZE * 2, '*') }}};
            var getter = {{{ makeGetValue('propPtr', POINTER_SIZE * 3, '*') }}};
            var setter = {{{ makeGetValue('propPtr', POINTER_SIZE * 4, '*') }}};
            var value = {{{ makeGetValue('propPtr', POINTER_SIZE * 5, '*') }}};
            attributes = {{{ makeGetValue('propPtr', POINTER_SIZE * 6, POINTER_WASM_TYPE) }}};
            {{{ from64('attributes') }}};
            var data = {{{ makeGetValue('propPtr', POINTER_SIZE * 7, '*') }}};
            if (utf8Name) {
                propertyName = emnapiString.UTF8ToString(utf8Name, -1);
            }
            else {
                if (!name_2) {
                    return envObject.setLastError(4 /* napi_status.napi_name_expected */);
                }
                propertyName = emnapiCtx.handleStore.get(name_2).value;
                if (typeof propertyName !== 'string' && typeof propertyName !== 'symbol') {
                    return envObject.setLastError(4 /* napi_status.napi_name_expected */);
                }
            }
            emnapiDefineProperty(envObject, maybeObject, propertyName, method, getter, setter, value, attributes, data);
        }
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipp */
function _napi_object_freeze(env, object) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        var maybeObject = h.value;
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        Object.freeze(maybeObject);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipp */
function _napi_object_seal(env, object) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(object);
        var maybeObject = h.value;
        if (!(h.isObject() || h.isFunction())) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        Object.seal(maybeObject);
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/* eslint-disable @typescript-eslint/indent */
/** @__sig ippp */
function _napi_run_script(env, script, result) {
    var status;
#if DYNAMIC_EXECUTION
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var value;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!script)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var v8Script = emnapiCtx.handleStore.get(script);
        if (!v8Script.isString()) {
            return envObject.setLastError(3 /* napi_status.napi_string_expected */);
        }
        var g = emnapiCtx.handleStore.get(5 /* GlobalHandle.GLOBAL */).value;
        var ret = g.eval(v8Script.value);
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        value = envObject.ensureHandleId(ret);
        {{{ makeSetValue('result', 0, 'value', '*') }}};
        status = envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
#else
    status = _napi_set_last_error(env, 9 /* napi_status.napi_generic_failure */, 0, 0);
#endif
    return status;
}
/* eslint-disable @typescript-eslint/indent */
/**
 * @__deps malloc
 * @__deps free
 * @__postset
 * ```
 * emnapiTSFN.init();
 * ```
 */
var emnapiTSFN = {
    offset: {
        __size__: 0,
        /* napi_ref */ resource: 0,
        /* double */ async_id: 0,
        /* double */ trigger_async_id: 0,
        /* size_t */ queue_size: 0,
        /* bool */ is_some: 0,
        /* void* */ queue: 0,
        /* size_t */ thread_count: 0,
        /* int32_t */ state: 0,
        /* atomic_uchar */ dispatch_state: 0,
        /* void* */ context: 0,
        /* size_t */ max_queue_size: 0,
        /* napi_ref */ ref: 0,
        /* napi_env */ env: 0,
        /* void* */ finalize_data: 0,
        /* napi_finalize */ finalize_cb: 0,
        /* napi_threadsafe_function_call_js */ call_js_cb: 0,
        /* bool */ handles_closing: 0,
        /* bool */ async_ref: 0,
        /* int32_t */ mutex: 0,
        /* int32_t */ cond: 0
    },
    init: function () {
#if MEMORY64
        emnapiTSFN.offset.__size__ = 320 /* NapiTSFNOffset64.__size__ */;
        emnapiTSFN.offset.resource = 0 /* NapiTSFNOffset64.async_resource_resource */;
        emnapiTSFN.offset.async_id = 8 /* NapiTSFNOffset64.async_resource_async_context_async_id */;
        emnapiTSFN.offset.trigger_async_id = 16 /* NapiTSFNOffset64.async_resource_async_context_trigger_async_id */;
        emnapiTSFN.offset.queue_size = 88 /* NapiTSFNOffset64.queue_size */;
        emnapiTSFN.offset.is_some = 24 /* NapiTSFNOffset64.async_resource_is_some */;
        emnapiTSFN.offset.queue = 96 /* NapiTSFNOffset64.queue */;
        emnapiTSFN.offset.thread_count = 240 /* NapiTSFNOffset64.thread_count */;
        emnapiTSFN.offset.state = 248 /* NapiTSFNOffset64.state */;
        emnapiTSFN.offset.dispatch_state = 252 /* NapiTSFNOffset64.dispatch_state */;
        emnapiTSFN.offset.context = 256 /* NapiTSFNOffset64.context */;
        emnapiTSFN.offset.max_queue_size = 264 /* NapiTSFNOffset64.max_queue_size */;
        emnapiTSFN.offset.ref = 272 /* NapiTSFNOffset64.ref */;
        emnapiTSFN.offset.env = 280 /* NapiTSFNOffset64.env */;
        emnapiTSFN.offset.finalize_data = 288 /* NapiTSFNOffset64.finalize_data */;
        emnapiTSFN.offset.finalize_cb = 296 /* NapiTSFNOffset64.finalize_cb */;
        emnapiTSFN.offset.call_js_cb = 304 /* NapiTSFNOffset64.call_js_cb */;
        emnapiTSFN.offset.handles_closing = 312 /* NapiTSFNOffset64.handles_closing */;
        emnapiTSFN.offset.async_ref = 316 /* NapiTSFNOffset64.async_ref */;
        emnapiTSFN.offset.mutex = 32 /* NapiTSFNOffset64.mutex */;
        emnapiTSFN.offset.cond = 80 /* NapiTSFNOffset64.cond */;
#else
        emnapiTSFN.offset.__size__ = 184 /* NapiTSFNOffset32.__size__ */;
        emnapiTSFN.offset.resource = 0 /* NapiTSFNOffset32.async_resource_resource */;
        emnapiTSFN.offset.async_id = 8 /* NapiTSFNOffset32.async_resource_async_context_async_id */;
        emnapiTSFN.offset.trigger_async_id = 16 /* NapiTSFNOffset32.async_resource_async_context_trigger_async_id */;
        emnapiTSFN.offset.queue_size = 60 /* NapiTSFNOffset32.queue_size */;
        emnapiTSFN.offset.is_some = 24 /* NapiTSFNOffset32.async_resource_is_some */;
        emnapiTSFN.offset.queue = 64 /* NapiTSFNOffset32.queue */;
        emnapiTSFN.offset.thread_count = 136 /* NapiTSFNOffset32.thread_count */;
        emnapiTSFN.offset.state = 140 /* NapiTSFNOffset32.state */;
        emnapiTSFN.offset.dispatch_state = 144 /* NapiTSFNOffset32.dispatch_state */;
        emnapiTSFN.offset.context = 148 /* NapiTSFNOffset32.context */;
        emnapiTSFN.offset.max_queue_size = 152 /* NapiTSFNOffset32.max_queue_size */;
        emnapiTSFN.offset.ref = 156 /* NapiTSFNOffset32.ref */;
        emnapiTSFN.offset.env = 160 /* NapiTSFNOffset32.env */;
        emnapiTSFN.offset.finalize_data = 164 /* NapiTSFNOffset32.finalize_data */;
        emnapiTSFN.offset.finalize_cb = 168 /* NapiTSFNOffset32.finalize_cb */;
        emnapiTSFN.offset.call_js_cb = 172 /* NapiTSFNOffset32.call_js_cb */;
        emnapiTSFN.offset.handles_closing = 176 /* NapiTSFNOffset32.handles_closing */;
        emnapiTSFN.offset.async_ref = 180 /* NapiTSFNOffset32.async_ref */;
        emnapiTSFN.offset.mutex = 32 /* NapiTSFNOffset32.mutex */;
        emnapiTSFN.offset.cond = 56 /* NapiTSFNOffset32.cond */;
#endif
        emnapiTSFN.offset.mutex = emnapiTSFN.offset.mutex + 4;
        if (typeof PThread !== 'undefined') {
            PThread.unusedWorkers.forEach(emnapiTSFN.addListener);
            PThread.runningWorkers.forEach(emnapiTSFN.addListener);
            var __original_getNewWorker_1 = PThread.getNewWorker;
            PThread.getNewWorker = function () {
                var r = __original_getNewWorker_1.apply(this, arguments);
                emnapiTSFN.addListener(r);
                return r;
            };
        }
    },
    addListener: function (worker) {
        if (!worker)
            return false;
        if (worker._emnapiTSFNListener)
            return true;
        var handler = function (e) {
            var data = ENVIRONMENT_IS_NODE ? e : e.data;
            var __emnapi__ = data.__emnapi__;
            if (__emnapi__) {
                var type = __emnapi__.type;
                var payload = __emnapi__.payload;
                if (type === 'tsfn-send') {
                    emnapiTSFN.dispatch(payload.tsfn);
                }
            }
        };
        var dispose = function () {
            if (ENVIRONMENT_IS_NODE) {
                worker.off('message', handler);
            }
            else {
                worker.removeEventListener('message', handler, false);
            }
            delete worker._emnapiTSFNListener;
        };
        worker._emnapiTSFNListener = { handler: handler, dispose: dispose };
        if (ENVIRONMENT_IS_NODE) {
            worker.on('message', handler);
        }
        else {
            worker.addEventListener('message', handler, false);
        }
        return true;
    },
    initQueue: function (func) {
        var size = 2 * {{{ POINTER_SIZE }}};
        var queue = _malloc({{{ to64('size') }}});
        if (!queue)
            return false;
        new Uint8Array(wasmMemory.buffer, queue, size).fill(0);
        emnapiTSFN.storeSizeTypeValue(func + emnapiTSFN.offset.queue, queue, false);
        return true;
    },
    destroyQueue: function (func) {
        var queue = emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.queue, false);
        if (queue) {
            _free({{{ to64('queue') }}});
        }
    },
    pushQueue: function (func, data) {
        var queue = emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.queue, false);
        var head = emnapiTSFN.loadSizeTypeValue(queue, false);
        var tail = emnapiTSFN.loadSizeTypeValue(queue + {{{ POINTER_SIZE }}}, false);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var size = 2 * {{{ POINTER_SIZE }}};
        var node = _malloc({{{ to64('size') }}});
        if (!node)
            throw new Error('OOM');
        emnapiTSFN.storeSizeTypeValue(node, data, false);
        emnapiTSFN.storeSizeTypeValue(node + {{{ POINTER_SIZE }}}, 0, false);
        if (head === 0 && tail === 0) {
            emnapiTSFN.storeSizeTypeValue(queue, node, false);
            emnapiTSFN.storeSizeTypeValue(queue + {{{ POINTER_SIZE }}}, node, false);
        }
        else {
            emnapiTSFN.storeSizeTypeValue(tail + {{{ POINTER_SIZE }}}, node, false);
            emnapiTSFN.storeSizeTypeValue(queue + {{{ POINTER_SIZE }}}, node, false);
        }
        emnapiTSFN.addQueueSize(func);
    },
    shiftQueue: function (func) {
        var queue = emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.queue, false);
        var head = emnapiTSFN.loadSizeTypeValue(queue, false);
        if (head === 0)
            return 0;
        var node = head;
        var next = emnapiTSFN.loadSizeTypeValue(head + {{{ POINTER_SIZE }}}, false);
        emnapiTSFN.storeSizeTypeValue(queue, next, false);
        if (next === 0) {
            emnapiTSFN.storeSizeTypeValue(queue + {{{ POINTER_SIZE }}}, 0, false);
        }
        emnapiTSFN.storeSizeTypeValue(node + {{{ POINTER_SIZE }}}, 0, false);
        var value = emnapiTSFN.loadSizeTypeValue(node, false);
        _free({{{ to64('node') }}});
        emnapiTSFN.subQueueSize(func);
        return value;
    },
    push: function (func, data, mode) {
        var mutex = emnapiTSFN.getMutex(func);
        var cond = emnapiTSFN.getCond(func);
        var waitCondition = function () {
            var queueSize = emnapiTSFN.getQueueSize(func);
            var maxSize = emnapiTSFN.getMaxQueueSize(func);
            return queueSize >= maxSize && maxSize > 0 && emnapiTSFN.getState(func) === 0 /* State.kOpen */;
        };
        var isBrowserMain = typeof window !== 'undefined' && typeof document !== 'undefined' && !ENVIRONMENT_IS_NODE;
        var shouldDelete = false;
        var ret = mutex.execute(function () {
            while (waitCondition()) {
                if (mode === 0 /* napi_threadsafe_function_call_mode.napi_tsfn_nonblocking */) {
                    return 15 /* napi_status.napi_queue_full */;
                }
                /**
                 * Browser JS main thread can not use `Atomics.wait`
                 *
                 * Related:
                 * https://github.com/nodejs/node/pull/32689
                 * https://github.com/nodejs/node/pull/33453
                 */
                if (isBrowserMain) {
                    return 21 /* napi_status.napi_would_deadlock */;
                }
                cond.wait();
            }
            if (emnapiTSFN.getState(func) === 0 /* State.kOpen */) {
                emnapiTSFN.pushQueue(func, data);
                emnapiTSFN.send(func);
                return 0 /* napi_status.napi_ok */;
            }
            if (emnapiTSFN.getThreadCount(func) === 0) {
                return 1 /* napi_status.napi_invalid_arg */;
            }
            emnapiTSFN.subThreadCount(func);
            if (!(emnapiTSFN.getState(func) === 2 /* State.kClosed */ && emnapiTSFN.getThreadCount(func) === 0)) {
                return 16 /* napi_status.napi_closing */;
            }
            shouldDelete = true;
            return 16 /* napi_status.napi_closing */;
        });
        if (shouldDelete) {
            emnapiTSFN.destroy(func);
        }
        return ret;
    },
    getMutex: function (func) {
        var index = func + emnapiTSFN.offset.mutex;
        var mutex = {
            lock: function () {
                var isBrowserMain = typeof window !== 'undefined' && typeof document !== 'undefined' && !ENVIRONMENT_IS_NODE;
                var i32a = new Int32Array(wasmMemory.buffer, index, 1);
                if (isBrowserMain) {
                    while (true) {
                        var oldValue = Atomics.compareExchange(i32a, 0, 0, 10);
                        if (oldValue === 0) {
                            return;
                        }
                    }
                }
                else {
                    while (true) {
                        var oldValue = Atomics.compareExchange(i32a, 0, 0, 10);
                        if (oldValue === 0) {
                            return;
                        }
                        Atomics.wait(i32a, 0, 10);
                    }
                }
            },
            /* lockAsync () {
              return new Promise<void>(resolve => {
                const again = (): void => { fn() }
                const fn = (): void => {
                  const i32a = new Int32Array(wasmMemory.buffer, index, 1)
                  const oldValue = Atomics.compareExchange(i32a, 0, 0, 10)
                  if (oldValue === 0) {
                    resolve()
                    return
                  }
                  (Atomics as any).waitAsync(i32a, 0, 10).value.then(again)
                }
                fn()
              })
            }, */
            unlock: function () {
                var i32a = new Int32Array(wasmMemory.buffer, index, 1);
                var oldValue = Atomics.compareExchange(i32a, 0, 10, 0);
                if (oldValue !== 10) {
                    throw new Error('Tried to unlock while not holding the mutex');
                }
                Atomics.notify(i32a, 0, 1);
            },
            execute: function (fn) {
                mutex.lock();
                try {
                    return fn();
                }
                finally {
                    mutex.unlock();
                }
            } /* ,
            executeAsync<T> (fn: () => Promise<T>): Promise<T> {
              return mutex.lockAsync().then(() => {
                const r = fn()
                mutex.unlock()
                return r
              }, (err) => {
                mutex.unlock()
                throw err
              })
            } */
        };
        return mutex;
    },
    getCond: function (func) {
        var index = func + emnapiTSFN.offset.cond;
        var mutex = emnapiTSFN.getMutex(func);
        var cond = {
            wait: function () {
                var i32a = new Int32Array(wasmMemory.buffer, index, 1);
                var value = Atomics.load(i32a, 0);
                mutex.unlock();
                Atomics.wait(i32a, 0, value);
                mutex.lock();
            },
            /* waitAsync () {
              const i32a = new Int32Array(wasmMemory.buffer, index, 1)
              const value = Atomics.load(i32a, 0)
              mutex.unlock()
              const lock = (): Promise<void> => mutex.lockAsync()
              try {
                return (Atomics as any).waitAsync(i32a, 0, value).value.then(lock, lock)
              } catch (err) {
                return lock()
              }
            }, */
            signal: function () {
                var i32a = new Int32Array(wasmMemory.buffer, index, 1);
                Atomics.add(i32a, 0, 1);
                Atomics.notify(i32a, 0, 1);
            }
        };
        return cond;
    },
    getQueueSize: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.queue_size, true);
    },
    addQueueSize: function (func) {
        var offset = emnapiTSFN.offset.queue_size;
        var arr, index;
#if MEMORY64
        arr = new BigUint64Array(wasmMemory.buffer);
        index = (func + offset) >> 3;
#else
        arr = new Uint32Array(wasmMemory.buffer);
        index = (func + offset) >> 2;
#endif
        Atomics.add(arr, index, {{{ to64('1') }}});
    },
    subQueueSize: function (func) {
        var offset = emnapiTSFN.offset.queue_size;
        var arr, index;
#if MEMORY64
        arr = new BigUint64Array(wasmMemory.buffer);
        index = (func + offset) >> 3;
#else
        arr = new Uint32Array(wasmMemory.buffer);
        index = (func + offset) >> 2;
#endif
        Atomics.sub(arr, index, {{{ to64('1') }}});
    },
    getThreadCount: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.thread_count, true);
    },
    addThreadCount: function (func) {
        var offset = emnapiTSFN.offset.thread_count;
        var arr, index;
#if MEMORY64
        arr = new BigUint64Array(wasmMemory.buffer);
        index = (func + offset) >> 3;
#else
        arr = new Uint32Array(wasmMemory.buffer);
        index = (func + offset) >> 2;
#endif
        Atomics.add(arr, index, {{{ to64('1') }}});
    },
    subThreadCount: function (func) {
        var offset = emnapiTSFN.offset.thread_count;
        var arr, index;
#if MEMORY64
        arr = new BigUint64Array(wasmMemory.buffer);
        index = (func + offset) >> 3;
#else
        arr = new Uint32Array(wasmMemory.buffer);
        index = (func + offset) >> 2;
#endif
        Atomics.sub(arr, index, {{{ to64('1') }}});
    },
    getState: function (func) {
        return Atomics.load(new Int32Array(wasmMemory.buffer), (func + emnapiTSFN.offset.state) >> 2);
    },
    setState: function (func, value) {
        Atomics.store(new Int32Array(wasmMemory.buffer), (func + emnapiTSFN.offset.state) >> 2, value);
    },
    getHandlesClosing: function (func) {
        return Atomics.load(new Int8Array(wasmMemory.buffer), (func + emnapiTSFN.offset.handles_closing));
    },
    setHandlesClosing: function (func, value) {
        Atomics.store(new Int8Array(wasmMemory.buffer), (func + emnapiTSFN.offset.handles_closing), value);
    },
    getDispatchState: function (func) {
        return Atomics.load(new Uint32Array(wasmMemory.buffer), (func + emnapiTSFN.offset.dispatch_state) >> 2);
    },
    getContext: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.context, false);
    },
    getMaxQueueSize: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.max_queue_size, true);
    },
    getEnv: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.env, false);
    },
    getCallJSCb: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.call_js_cb, false);
    },
    getRef: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.ref, false);
    },
    getResource: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.resource, false);
    },
    getFinalizeCb: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.finalize_cb, false);
    },
    getFinalizeData: function (func) {
        return emnapiTSFN.loadSizeTypeValue(func + emnapiTSFN.offset.finalize_data, false);
    },
    loadSizeTypeValue: function (offset, unsigned) {
        var ret;
        var arr;
        if (unsigned) {
#if MEMORY64
            arr = new BigUint64Array(wasmMemory.buffer);
            ret = Number(Atomics.load(arr, offset >> 3));
#else
            arr = new Uint32Array(wasmMemory.buffer);
            ret = Atomics.load(arr, offset >> 2);
#endif
            return ret;
        }
        else {
#if MEMORY64
            arr = new BigInt64Array(wasmMemory.buffer);
            ret = Number(Atomics.load(arr, offset >> 3));
#else
            arr = new Int32Array(wasmMemory.buffer);
            ret = Atomics.load(arr, offset >> 2);
#endif
            return ret;
        }
    },
    storeSizeTypeValue: function (offset, value, unsigned) {
        var arr;
        if (unsigned) {
#if MEMORY64
            arr = new BigUint64Array(wasmMemory.buffer);
            Atomics.store(arr, offset >> 3, BigInt(value));
#else
            arr = new Uint32Array(wasmMemory.buffer);
            Atomics.store(arr, offset >> 2, value);
#endif
            return undefined;
        }
        else {
#if MEMORY64
            arr = new BigInt64Array(wasmMemory.buffer);
            Atomics.store(arr, offset >> 3, BigInt(value >>> 0));
#else
            arr = new Int32Array(wasmMemory.buffer);
            Atomics.store(arr, offset >> 2, value >>> 0);
#endif
            return undefined;
        }
    },
    releaseResources: function (func) {
        if (emnapiTSFN.getState(func) !== 2 /* State.kClosed */) {
            emnapiTSFN.setState(func, 2 /* State.kClosed */);
            var env = emnapiTSFN.getEnv(func);
            var envObject = emnapiCtx.envStore.get(env);
            var ref = emnapiTSFN.getRef(func);
            if (ref) {
                emnapiCtx.refStore.get(ref).dispose();
            }
            var resource = emnapiTSFN.getResource(func);
            emnapiCtx.refStore.get(resource).dispose();
            {{{ makeSetValue('func', 'emnapiTSFN.offset.is_some', '0', 'i8') }}};
            emnapiCtx.removeCleanupHook(envObject, emnapiTSFN.cleanup, func);
            envObject.unref();
            var asyncRefOffset = (func + emnapiTSFN.offset.async_ref) >> 2;
            var arr = new Uint32Array(wasmMemory.buffer);
            if (Atomics.load(arr, asyncRefOffset) > 0) {
                Atomics.store(arr, asyncRefOffset, 0);
                __emnapi_runtime_keepalive_pop();
                emnapiCtx.decreaseWaitingRequestCounter();
            }
            if (emnapiNodeBinding) {
                var view = new DataView(wasmMemory.buffer);
                var asyncId = view.getFloat64(func + emnapiTSFN.offset.async_id, true);
                var triggerAsyncId = view.getFloat64(func + emnapiTSFN.offset.trigger_async_id, true);
                __emnapi_node_emit_async_destroy(asyncId, triggerAsyncId);
            }
        }
    },
    destroy: function (func) {
        emnapiTSFN.destroyQueue(func);
        emnapiTSFN.releaseResources(func);
        _free({{{ to64('func') }}});
    },
    emptyQueue: function (func) {
        var drainQueue = [];
        emnapiTSFN.getMutex(func).execute(function () {
            while (emnapiTSFN.getQueueSize(func) > 0) {
                drainQueue.push(emnapiTSFN.shiftQueue(func));
            }
        });
        var callJsCb = emnapiTSFN.getCallJSCb(func);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var context = emnapiTSFN.getContext(func);
        var data;
        for (var i = 0; i < drainQueue.length; i++) {
            // eslint-disable-next-line @typescript-eslint/no-unused-vars
            data = drainQueue[i];
            if (callJsCb) {
                {{{ makeDynCall('vpppp', 'callJsCb') }}}({{{ to64('0') }}}, {{{ to64('0') }}}, {{{ to64('context') }}}, {{{ to64('data') }}});
            }
        }
    },
    maybeDelete: function (func) {
        var shouldDelete = false;
        emnapiTSFN.getMutex(func).execute(function () {
            if (emnapiTSFN.getThreadCount(func) > 0) {
                emnapiTSFN.releaseResources(func);
            }
            else {
                shouldDelete = true;
            }
        });
        if (shouldDelete) {
            emnapiTSFN.destroy(func);
        }
    },
    finalize: function (func) {
        var env = emnapiTSFN.getEnv(func);
        var envObject = emnapiCtx.envStore.get(env);
        emnapiCtx.openScope(envObject);
        var finalize = emnapiTSFN.getFinalizeCb(func);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var data = emnapiTSFN.getFinalizeData(func);
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var context = emnapiTSFN.getContext(func);
        var f = function () {
            envObject.callFinalizerInternal(0, {{{ to64('finalize') }}}, {{{ to64('data') }}}, {{{ to64('context') }}});
        };
        try {
            emnapiTSFN.emptyQueue(func);
            if (finalize) {
                if (emnapiNodeBinding) {
                    var resource = emnapiTSFN.getResource(func);
                    var resource_value = emnapiCtx.refStore.get(resource).get();
                    var resourceObject = emnapiCtx.handleStore.get(resource_value).value;
                    var view = new DataView(wasmMemory.buffer);
                    var asyncId = view.getFloat64(func + emnapiTSFN.offset.async_id, true);
                    var triggerAsyncId = view.getFloat64(func + emnapiTSFN.offset.trigger_async_id, true);
                    emnapiNodeBinding.node.makeCallback(resourceObject, f, [], {
                        asyncId: asyncId,
                        triggerAsyncId: triggerAsyncId
                    });
                }
                else {
                    f();
                }
            }
            emnapiTSFN.maybeDelete(func);
        }
        finally {
            emnapiCtx.closeScope(envObject);
        }
    },
    cleanup: function (func) {
        emnapiTSFN.closeHandlesAndMaybeDelete(func, 1);
    },
    closeHandlesAndMaybeDelete: function (func, set_closing) {
        var env = emnapiTSFN.getEnv(func);
        var envObject = emnapiCtx.envStore.get(env);
        emnapiCtx.openScope(envObject);
        try {
            if (set_closing) {
                emnapiTSFN.getMutex(func).execute(function () {
                    emnapiTSFN.setState(func, 1 /* State.kClosing */);
                    if (emnapiTSFN.getMaxQueueSize(func) > 0) {
                        emnapiTSFN.getCond(func).signal();
                    }
                });
            }
            if (emnapiTSFN.getHandlesClosing(func)) {
                return;
            }
            emnapiTSFN.setHandlesClosing(func, 1);
            emnapiCtx.feature.setImmediate(function () {
                emnapiTSFN.finalize(func);
            });
        }
        finally {
            emnapiCtx.closeScope(envObject);
        }
    },
    dispatchOne: function (func) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        var data = 0;
        var popped_value = false;
        var has_more = false;
        var mutex = emnapiTSFN.getMutex(func);
        var cond = emnapiTSFN.getCond(func);
        mutex.execute(function () {
            if (emnapiTSFN.getState(func) === 0 /* State.kOpen */) {
                var size = emnapiTSFN.getQueueSize(func);
                if (size > 0) {
                    data = emnapiTSFN.shiftQueue(func);
                    popped_value = true;
                    var maxQueueSize = emnapiTSFN.getMaxQueueSize(func);
                    if (size === maxQueueSize && maxQueueSize > 0) {
                        cond.signal();
                    }
                    size--;
                }
                if (size === 0) {
                    if (emnapiTSFN.getThreadCount(func) === 0) {
                        emnapiTSFN.setState(func, 1 /* State.kClosing */);
                        if (emnapiTSFN.getMaxQueueSize(func) > 0) {
                            cond.signal();
                        }
                        emnapiTSFN.closeHandlesAndMaybeDelete(func, 0);
                    }
                }
                else {
                    has_more = true;
                }
            }
            else {
                emnapiTSFN.closeHandlesAndMaybeDelete(func, 0);
            }
        });
        if (popped_value) {
            var env = emnapiTSFN.getEnv(func);
            var envObject_1 = emnapiCtx.envStore.get(env);
            emnapiCtx.openScope(envObject_1);
            var f = function () {
                envObject_1.callbackIntoModule(false, function () {
                    var callJsCb = emnapiTSFN.getCallJSCb(func);
                    var ref = emnapiTSFN.getRef(func);
                    var js_callback = ref ? emnapiCtx.refStore.get(ref).get() : 0;
                    if (callJsCb) {
                        // eslint-disable-next-line @typescript-eslint/no-unused-vars
                        var context = emnapiTSFN.getContext(func);
                        {{{ makeDynCall('vpppp', 'callJsCb') }}}({{{ to64('env') }}}, {{{ to64('js_callback') }}}, {{{ to64('context') }}}, {{{ to64('data') }}});
                    }
                    else {
                        var jsCallback = js_callback ? emnapiCtx.handleStore.get(js_callback).value : null;
                        if (typeof jsCallback === 'function') {
                            jsCallback();
                        }
                    }
                });
            };
            try {
                if (emnapiNodeBinding) {
                    var resource = emnapiTSFN.getResource(func);
                    var resource_value = emnapiCtx.refStore.get(resource).get();
                    var resourceObject = emnapiCtx.handleStore.get(resource_value).value;
                    var view = new DataView(wasmMemory.buffer);
                    emnapiNodeBinding.node.makeCallback(resourceObject, f, [], {
                        asyncId: view.getFloat64(func + emnapiTSFN.offset.async_id, true),
                        triggerAsyncId: view.getFloat64(func + emnapiTSFN.offset.trigger_async_id, true)
                    });
                }
                else {
                    f();
                }
            }
            finally {
                emnapiCtx.closeScope(envObject_1);
            }
        }
        return has_more;
    },
    dispatch: function (func) {
        var has_more = true;
        var iterations_left = 1000;
        var ui32a = new Uint32Array(wasmMemory.buffer);
        var index = (func + emnapiTSFN.offset.dispatch_state) >> 2;
        while (has_more && --iterations_left !== 0) {
            Atomics.store(ui32a, index, 1);
            has_more = emnapiTSFN.dispatchOne(func);
            if (Atomics.exchange(ui32a, index, 0) !== 1) {
                has_more = true;
            }
        }
        if (has_more) {
            emnapiTSFN.send(func);
        }
    },
    send: function (func) {
        var current_state = Atomics.or(new Uint32Array(wasmMemory.buffer), (func + emnapiTSFN.offset.dispatch_state) >> 2, 1 << 1);
        if ((current_state & 1) === 1) {
            return;
        }
        if ((typeof ENVIRONMENT_IS_PTHREAD !== 'undefined') && ENVIRONMENT_IS_PTHREAD) {
            postMessage({
                __emnapi__: {
                    type: 'tsfn-send',
                    payload: {
                        tsfn: func
                    }
                }
            });
        }
        else {
            emnapiCtx.feature.setImmediate(function () {
                emnapiTSFN.dispatch(func);
            });
        }
    }
};
/** @__sig ippppppppppp */
function _napi_create_threadsafe_function(env, func, async_resource, async_resource_name, max_queue_size, initial_thread_count, thread_finalize_data, thread_finalize_cb, context, call_js_cb, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!async_resource_name)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    {{{ from64('max_queue_size') }}};
    {{{ from64('initial_thread_count') }}};
    {{{ from64('env') }}};
    {{{ from64('thread_finalize_data') }}};
    {{{ from64('thread_finalize_cb') }}};
    {{{ from64('context') }}};
    {{{ from64('call_js_cb') }}};
    max_queue_size = max_queue_size >>> 0;
    initial_thread_count = initial_thread_count >>> 0;
    if (initial_thread_count === 0) {
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var ref = 0;
    {{{ from64('func') }}};
    if (!func) {
        if (!call_js_cb)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    else {
        var funcValue = emnapiCtx.handleStore.get(func).value;
        if (typeof funcValue !== 'function') {
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        }
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        ref = emnapiCtx.createReference(envObject, func, 1, 1 /* ReferenceOwnership.kUserland */).id;
    }
    var asyncResourceObject;
    if (async_resource) {
        asyncResourceObject = emnapiCtx.handleStore.get(async_resource).value;
        if (asyncResourceObject == null) {
            return envObject.setLastError(2 /* napi_status.napi_object_expected */);
        }
        asyncResourceObject = Object(asyncResourceObject);
    }
    else {
        asyncResourceObject = {};
    }
    var resource = envObject.ensureHandleId(asyncResourceObject);
    var asyncResourceName = emnapiCtx.handleStore.get(async_resource_name).value;
    if (typeof asyncResourceName === 'symbol') {
        return envObject.setLastError(3 /* napi_status.napi_string_expected */);
    }
    asyncResourceName = String(asyncResourceName);
    var resource_name = envObject.ensureHandleId(asyncResourceName);
    // tsfn create
    var sizeofTSFN = emnapiTSFN.offset.__size__;
    // eslint-disable-next-line prefer-const
    var tsfn = _malloc({{{ to64('sizeofTSFN') }}});
    if (!tsfn)
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    {{{ from64('tsfn') }}};
    new Uint8Array(wasmMemory.buffer).subarray(tsfn, tsfn + sizeofTSFN).fill(0);
    var resourceRef = emnapiCtx.createReference(envObject, resource, 1, 1 /* ReferenceOwnership.kUserland */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var resource_ = resourceRef.id;
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.resource', 'resource_', '*') }}};
    if (!emnapiTSFN.initQueue(tsfn)) {
        _free({{{ to64('tsfn') }}});
        resourceRef.dispose();
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    __emnapi_node_emit_async_init(resource, resource_name, -1, tsfn + emnapiTSFN.offset.async_id);
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.is_some', '1', 'i8') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.thread_count', 'initial_thread_count', SIZE_TYPE) }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.context', 'context', '*') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.max_queue_size', 'max_queue_size', SIZE_TYPE) }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.ref', 'ref', '*') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.env', 'env', '*') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.finalize_data', 'thread_finalize_data', '*') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.finalize_cb', 'thread_finalize_cb', '*') }}};
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.call_js_cb', 'call_js_cb', '*') }}};
    emnapiCtx.addCleanupHook(envObject, emnapiTSFN.cleanup, tsfn);
    envObject.ref();
    __emnapi_runtime_keepalive_push();
    emnapiCtx.increaseWaitingRequestCounter();
    {{{ makeSetValue('tsfn', 'emnapiTSFN.offset.async_ref', '1', 'u32') }}};
    {{{ from64('result') }}};
    {{{ makeSetValue('result', 0, 'tsfn', '*') }}};
    return envObject.clearLastError();
}
/** @__sig ipp */
function _napi_get_threadsafe_function_context(func, result) {
    if (!func || !result) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var context = emnapiTSFN.getContext(func);
    {{{ makeSetValue('result', 0, 'context', '*') }}};
    return 0 /* napi_status.napi_ok */;
}
/** @__sig ippi */
function _napi_call_threadsafe_function(func, data, mode) {
    if (!func) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    {{{ from64('data') }}};
    return emnapiTSFN.push(func, data, mode);
}
/** @__sig ip */
function _napi_acquire_threadsafe_function(func) {
    if (!func) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    var mutex = emnapiTSFN.getMutex(func);
    return mutex.execute(function () {
        if (emnapiTSFN.getState(func) === 0 /* State.kOpen */) {
            emnapiTSFN.addThreadCount(func);
            return 0 /* napi_status.napi_ok */;
        }
        return 16 /* napi_status.napi_closing */;
    });
}
/** @__sig ipi */
function _napi_release_threadsafe_function(func, mode) {
    if (!func) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    var mutex = emnapiTSFN.getMutex(func);
    var cond = emnapiTSFN.getCond(func);
    var shouldDelete = false;
    var ret = mutex.execute(function () {
        if (emnapiTSFN.getThreadCount(func) === 0) {
            return 1 /* napi_status.napi_invalid_arg */;
        }
        emnapiTSFN.subThreadCount(func);
        if (emnapiTSFN.getThreadCount(func) === 0 || mode === 1 /* napi_threadsafe_function_release_mode.napi_tsfn_abort */) {
            if (emnapiTSFN.getState(func) === 0 /* State.kOpen */) {
                if (mode === 1 /* napi_threadsafe_function_release_mode.napi_tsfn_abort */) {
                    emnapiTSFN.setState(func, 1 /* State.kClosing */);
                }
                if (emnapiTSFN.getState(func) === 1 /* State.kClosing */ && emnapiTSFN.getMaxQueueSize(func) > 0) {
                    cond.signal();
                }
                emnapiTSFN.send(func);
            }
        }
        if (!(emnapiTSFN.getState(func) === 2 /* State.kClosed */ && emnapiTSFN.getThreadCount(func) === 0)) {
            return 0 /* napi_status.napi_ok */;
        }
        shouldDelete = true;
        return 0 /* napi_status.napi_ok */;
    });
    if (shouldDelete) {
        emnapiTSFN.destroy(func);
    }
    return ret;
}
/** @__sig ipp */
function _napi_unref_threadsafe_function(env, func) {
    if (!func) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    var asyncRefOffset = (func + emnapiTSFN.offset.async_ref) >> 2;
    var arr = new Uint32Array(wasmMemory.buffer);
    var currentValue = Atomics.load(arr, asyncRefOffset);
    if (currentValue > 0) {
        Atomics.store(arr, asyncRefOffset, currentValue - 1);
        if (currentValue === 1) {
            __emnapi_runtime_keepalive_pop();
            emnapiCtx.decreaseWaitingRequestCounter();
        }
    }
    return 0 /* napi_status.napi_ok */;
}
/** @__sig ipp */
function _napi_ref_threadsafe_function(env, func) {
    if (!func) {
        abort();
        return 1 /* napi_status.napi_invalid_arg */;
    }
    {{{ from64('func') }}};
    var asyncRefOffset = (func + emnapiTSFN.offset.async_ref) >> 2;
    var arr = new Uint32Array(wasmMemory.buffer);
    var currentValue = Atomics.load(arr, asyncRefOffset);
    if (!currentValue) {
        __emnapi_runtime_keepalive_push();
        emnapiCtx.increaseWaitingRequestCounter();
    }
    Atomics.store(arr, asyncRefOffset, currentValue + 1);
    return 0 /* napi_status.napi_ok */;
}
/** @__sig ippp */
function _napi_typeof(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var v = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    var r;
    if (v.isNumber()) {
        r = 3 /* napi_valuetype.napi_number */;
    }
    else if (v.isBigInt()) {
        r = 9 /* napi_valuetype.napi_bigint */;
    }
    else if (v.isString()) {
        r = 4 /* napi_valuetype.napi_string */;
    }
    else if (v.isFunction()) {
        // This test has to come before IsObject because IsFunction
        // implies IsObject
        r = 7 /* napi_valuetype.napi_function */;
    }
    else if (v.isExternal()) {
        // This test has to come before IsObject because IsExternal
        // implies IsObject
        r = 8 /* napi_valuetype.napi_external */;
    }
    else if (v.isObject()) {
        r = 6 /* napi_valuetype.napi_object */;
    }
    else if (v.isBoolean()) {
        r = 2 /* napi_valuetype.napi_boolean */;
    }
    else if (v.isUndefined()) {
        r = 0 /* napi_valuetype.napi_undefined */;
    }
    else if (v.isSymbol()) {
        r = 5 /* napi_valuetype.napi_symbol */;
    }
    else if (v.isNull()) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        r = 1 /* napi_valuetype.napi_null */;
    }
    else {
        // Should not get here unless V8 has added some new kind of value.
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    }
    {{{ makeSetValue('result', 0, 'r', 'i32') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_coerce_to_bool(env, value, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        {{{ from64('result') }}};
        v = handle.value ? 4 /* GlobalHandle.TRUE */ : 3 /* GlobalHandle.FALSE */;
        {{{ makeSetValue('result', 0, 'v', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_coerce_to_number(env, value, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (handle.isBigInt()) {
            throw new TypeError('Cannot convert a BigInt value to a number');
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        v = emnapiCtx.addToCurrentScope(Number(handle.value)).id;
        {{{ makeSetValue('result', 0, 'v', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_coerce_to_object(env, value, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (handle.value == null) {
            throw new TypeError('Cannot convert undefined or null to object');
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        v = envObject.ensureHandleId(Object(handle.value));
        {{{ makeSetValue('result', 0, 'v', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_coerce_to_string(env, value, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var v;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!value)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var handle = emnapiCtx.handleStore.get(value);
        if (handle.isSymbol()) {
            throw new TypeError('Cannot convert a Symbol value to a string');
        }
        {{{ from64('result') }}};
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        v = emnapiCtx.addToCurrentScope(String(handle.value)).id;
        {{{ makeSetValue('result', 0, 'v', '*') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipppp */
function _napi_instanceof(env, object, constructor, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!object)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!constructor)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        {{{ from64('result') }}};
        {{{ makeSetValue('result', 0, '0', 'i8') }}};
        var ctor = emnapiCtx.handleStore.get(constructor);
        if (!ctor.isFunction()) {
            return envObject.setLastError(5 /* napi_status.napi_function_expected */);
        }
        var val = emnapiCtx.handleStore.get(object).value;
        var ret = val instanceof ctor.value;
        r = ret ? 1 : 0;
        {{{ makeSetValue('result', 0, 'r', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ippp */
function _napi_is_array(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isArray() ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_arraybuffer(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isArrayBuffer() ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _node_api_is_sharedarraybuffer(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = ((typeof SharedArrayBuffer === 'function' && h.value instanceof SharedArrayBuffer) ||
        (Object.prototype.toString.call(h.value) === '[object SharedArrayBuffer]'))
        ? 1
        : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_date(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isDate() ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_error(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var val = emnapiCtx.handleStore.get(value).value;
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = (val instanceof Error) ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_typedarray(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isTypedArray() ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_buffer(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isBuffer(emnapiCtx.feature.Buffer) ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_dataview(env, value, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!value)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var h = emnapiCtx.handleStore.get(value);
    {{{ from64('result') }}};
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r = h.isDataView() ? 1 : 0;
    {{{ makeSetValue('result', 0, 'r', 'i8') }}};
    return envObject.clearLastError();
}
/** @__sig ipppp */
function _napi_strict_equals(env, lhs, rhs, result) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var r;
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!lhs)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!rhs)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var lv = emnapiCtx.handleStore.get(lhs).value;
        var rv = emnapiCtx.handleStore.get(rhs).value;
        {{{ from64('result') }}};
        r = (lv === rv) ? 1 : 0;
        {{{ makeSetValue('result', 0, 'r', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipp */
function _napi_detach_arraybuffer(env, arraybuffer) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!arraybuffer)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    var value = emnapiCtx.handleStore.get(arraybuffer).value;
    if (!(value instanceof ArrayBuffer)) {
        if (typeof SharedArrayBuffer === 'function' && (value instanceof SharedArrayBuffer)) {
            return envObject.setLastError(20 /* napi_status.napi_detachable_arraybuffer_expected */);
        }
        return envObject.setLastError(19 /* napi_status.napi_arraybuffer_expected */);
    }
    try {
        var MessageChannel_1 = emnapiCtx.feature.MessageChannel;
        var messageChannel = new MessageChannel_1();
        messageChannel.port1.postMessage(value, [value]);
    }
    catch (_) {
        return envObject.setLastError(9 /* napi_status.napi_generic_failure */);
    }
    return envObject.clearLastError();
}
/** @__sig ippp */
function _napi_is_detached_arraybuffer(env, arraybuffer, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    // @ts-expect-error
    var envObject = emnapiCtx.envStore.get(env);
    envObject.checkGCAccess();
    if (!envObject.tryCatch.isEmpty())
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    if (!envObject.canCallIntoJs())
        return envObject.setLastError(envObject.moduleApiVersion >= 10 ? 23 /* napi_status.napi_cannot_run_js */ : 10 /* napi_status.napi_pending_exception */);
    envObject.clearLastError();
    try {
        if (!arraybuffer)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        if (!result)
            return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
        var h = emnapiCtx.handleStore.get(arraybuffer);
        {{{ from64('result') }}};
        if (h.isArrayBuffer() && h.value.byteLength === 0) {
            try {
                // eslint-disable-next-line no-new
                new Uint8Array(h.value);
            }
            catch (_) {
                {{{ makeSetValue('result', 0, '1', 'i8') }}};
                return envObject.getReturnStatus();
            }
        }
        {{{ makeSetValue('result', 0, '0', 'i8') }}};
        return envObject.getReturnStatus();
    }
    catch (err) {
        envObject.tryCatch.setError(err);
        return envObject.setLastError(10 /* napi_status.napi_pending_exception */);
    }
}
/** @__sig ipp */
function _napi_get_version(env, result) {
    if (!env)
        return 1 /* napi_status.napi_invalid_arg */;
    var envObject = emnapiCtx.envStore.get(env);
    if (!result)
        return envObject.setLastError(1 /* napi_status.napi_invalid_arg */);
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    var NODE_API_SUPPORTED_VERSION_MAX = 10 /* Version.NODE_API_SUPPORTED_VERSION_MAX */;
    {{{ makeSetValue('result', 0, 'NODE_API_SUPPORTED_VERSION_MAX', 'u32') }}};
    return envObject.clearLastError();
}
(typeof addToLibrary === "function" ? addToLibrary : (...args) => mergeInto(LibraryManager.library, ...args))({
    $emnapiAddSendListener: emnapiAddSendListener,
    $emnapiAddSendListener__deps: ["$PThread"],
    $emnapiAddSendListener__postset: "PThread.unusedWorkers.forEach(emnapiAddSendListener);\nPThread.runningWorkers.forEach(emnapiAddSendListener);\n(function () {\n  var __original_getNewWorker = PThread.getNewWorker;\n  PThread.getNewWorker = function () {\n    var r = __original_getNewWorker.apply(this, arguments);\n    emnapiAddSendListener(r);\n    return r;\n  };\n})();",
    $emnapiAsyncWorkPoolSize: "0",
    $emnapiCtx: "undefined",
    $emnapiExternalMemory: emnapiExternalMemory,
    $emnapiExternalMemory__deps: ["$emnapiCtx", "malloc", "free"],
    $emnapiExternalMemory__postset: "emnapiExternalMemory.init();",
    $emnapiGetMemoryAddress: emnapiGetMemoryAddress,
    $emnapiGetMemoryAddress__deps: ["$emnapiExternalMemory"],
    $emnapiModule: emnapiModule,
    $emnapiNodeBinding: "undefined",
    $emnapiInit: emnapiInit,
    $emnapiInit__deps: ["$emnapiModule", "$emnapiCtx", "$emnapiNodeBinding", "$emnapiAsyncWorkPoolSize", "napi_register_wasm_v1", "node_api_module_get_api_version_v1"],
    $emnapiSetValueI64: emnapiSetValueI64,
    $emnapiSyncMemory: emnapiSyncMemory,
    $emnapiSyncMemory__deps: ["$emnapiExternalMemory"],
    _emnapi_async_destroy_js: __emnapi_async_destroy_js,
    _emnapi_async_destroy_js__deps: ["$emnapiNodeBinding"],
    _emnapi_async_destroy_js__sig: "ip",
    _emnapi_async_init_js: __emnapi_async_init_js,
    _emnapi_async_init_js__deps: ["$emnapiNodeBinding", "$emnapiCtx"],
    _emnapi_async_init_js__sig: "ippp",
    _emnapi_set_immediate: __emnapi_set_immediate,
    _emnapi_set_immediate__deps: ["$emnapiCtx"],
    _emnapi_set_immediate__sig: "vpp",
    _emnapi_next_tick: __emnapi_next_tick,
    _emnapi_next_tick__sig: "vpp",
    _emnapi_async_send_js: __emnapi_async_send_js,
    _emnapi_async_send_js__deps: ["_emnapi_set_immediate", "_emnapi_next_tick"],
    _emnapi_async_send_js__sig: "vipp",
    _emnapi_async_work_pool_size: __emnapi_async_work_pool_size,
    _emnapi_async_work_pool_size__deps: ["$emnapiAsyncWorkPoolSize"],
    _emnapi_async_work_pool_size__sig: "i",
    _emnapi_call_finalizer: __emnapi_call_finalizer,
    _emnapi_call_finalizer__deps: ["$emnapiCtx"],
    _emnapi_call_finalizer__sig: "vipppp",
    _emnapi_callback_into_module: __emnapi_callback_into_module,
    _emnapi_callback_into_module__deps: ["$emnapiCtx"],
    _emnapi_callback_into_module__sig: "vipppi",
    _emnapi_close_handle_scope: __emnapi_close_handle_scope,
    _emnapi_close_handle_scope__deps: ["$emnapiCtx", "$emnapiCtx"],
    _emnapi_close_handle_scope__sig: "vp",
    $emnapiString: emnapiString,
    $emnapiString__deps: ["$emnapiCtx"],
    $emnapiString__postset: "emnapiString.init();",
    $emnapiCreateFunction: emnapiCreateFunction,
    $emnapiCreateFunction__deps: ["$emnapiString", "$emnapiCtx"],
    _emnapi_create_function: __emnapi_create_function,
    _emnapi_create_function__deps: ["$emnapiCtx", "$emnapiCreateFunction"],
    _emnapi_create_function__sig: "ipppppp",
    _emnapi_ctx_decrease_waiting_request_counter: __emnapi_ctx_decrease_waiting_request_counter,
    _emnapi_ctx_decrease_waiting_request_counter__deps: ["$emnapiCtx"],
    _emnapi_ctx_decrease_waiting_request_counter__sig: "v",
    _emnapi_ctx_increase_waiting_request_counter: __emnapi_ctx_increase_waiting_request_counter,
    _emnapi_ctx_increase_waiting_request_counter__deps: ["$emnapiCtx"],
    _emnapi_ctx_increase_waiting_request_counter__sig: "v",
    _emnapi_env_check_gc_access: __emnapi_env_check_gc_access,
    _emnapi_env_check_gc_access__deps: ["$emnapiCtx"],
    _emnapi_env_check_gc_access__sig: "vp",
    _emnapi_env_ref: __emnapi_env_ref,
    _emnapi_env_ref__deps: ["$emnapiCtx"],
    _emnapi_env_ref__sig: "vp",
    _emnapi_env_unref: __emnapi_env_unref,
    _emnapi_env_unref__deps: ["$emnapiCtx"],
    _emnapi_env_unref__sig: "vp",
    _emnapi_get_filename: __emnapi_get_filename,
    _emnapi_get_filename__deps: ["$emnapiCtx", "$emnapiString"],
    _emnapi_get_filename__sig: "ippi",
    _emnapi_get_last_error_info: __emnapi_get_last_error_info,
    _emnapi_get_last_error_info__deps: ["$emnapiCtx"],
    _emnapi_get_last_error_info__sig: "vpppp",
    _emnapi_get_node_version: __emnapi_get_node_version,
    _emnapi_get_node_version__sig: "vppp",
    _emnapi_get_now: __emnapi_get_now,
    _emnapi_get_now__sig: "d",
    _emnapi_is_main_browser_thread: __emnapi_is_main_browser_thread,
    _emnapi_is_main_browser_thread__sig: "i",
    _emnapi_is_main_runtime_thread: __emnapi_is_main_runtime_thread,
    _emnapi_is_main_runtime_thread__sig: "i",
    _emnapi_node_emit_async_destroy: __emnapi_node_emit_async_destroy,
    _emnapi_node_emit_async_destroy__deps: ["$emnapiNodeBinding"],
    _emnapi_node_emit_async_destroy__sig: "vdd",
    _emnapi_node_emit_async_init: __emnapi_node_emit_async_init,
    _emnapi_node_emit_async_init__deps: ["$emnapiNodeBinding", "$emnapiCtx"],
    _emnapi_node_emit_async_init__sig: "vppdp",
    _emnapi_node_make_callback: __emnapi_node_make_callback,
    _emnapi_node_make_callback__deps: ["$emnapiNodeBinding", "$emnapiCtx"],
    _emnapi_node_make_callback__sig: "ipppppddp",
    _emnapi_open_handle_scope: __emnapi_open_handle_scope,
    _emnapi_open_handle_scope__deps: ["$emnapiCtx", "$emnapiCtx"],
    _emnapi_open_handle_scope__sig: "p",
    _emnapi_runtime_keepalive_pop: __emnapi_runtime_keepalive_pop,
    _emnapi_runtime_keepalive_pop__deps: ["$runtimeKeepalivePop"],
    _emnapi_runtime_keepalive_pop__sig: "v",
    _emnapi_runtime_keepalive_push: __emnapi_runtime_keepalive_push,
    _emnapi_runtime_keepalive_push__deps: ["$runtimeKeepalivePush"],
    _emnapi_runtime_keepalive_push__sig: "v",
    _emnapi_unwind: __emnapi_unwind,
    _emnapi_unwind__sig: "v",
    _emnapi_worker_ref: __emnapi_worker_ref,
    _emnapi_worker_ref__deps: ["$PThread"],
    _emnapi_worker_ref__sig: "vp",
    _emnapi_worker_unref: __emnapi_worker_unref,
    _emnapi_worker_unref__deps: ["$PThread"],
    _emnapi_worker_unref__sig: "vp",
    $emnapiGetHandle: emnapiGetHandle,
    $emnapiGetHandle__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_add_finalizer: _napi_add_finalizer,
    napi_add_finalizer__deps: ["$emnapiCtx", "$emnapiGetHandle"],
    napi_add_finalizer__sig: "ipppppp",
    emnapi_create_memory_view: _emnapi_create_memory_view,
    emnapi_create_memory_view__deps: ["$emnapiCtx", "$emnapiExternalMemory", "napi_add_finalizer"],
    emnapi_create_memory_view__sig: "ipippppp",
    emnapi_get_memory_address: _emnapi_get_memory_address,
    emnapi_get_memory_address__deps: ["$emnapiCtx", "$emnapiGetMemoryAddress"],
    emnapi_get_memory_address__sig: "ipppp",
    emnapi_get_module_object: _emnapi_get_module_object,
    emnapi_get_module_object__deps: ["$emnapiCtx"],
    emnapi_get_module_object__sig: "ipp",
    emnapi_get_module_property: _emnapi_get_module_property,
    emnapi_get_module_property__deps: ["$emnapiCtx", "$emnapiString"],
    emnapi_get_module_property__sig: "ippp",
    emnapi_get_runtime_version: _emnapi_get_runtime_version,
    emnapi_get_runtime_version__deps: ["$emnapiCtx"],
    emnapi_get_runtime_version__sig: "ipp",
    emnapi_is_node_binding_available: _emnapi_is_node_binding_available,
    emnapi_is_node_binding_available__deps: ["$emnapiNodeBinding"],
    emnapi_is_node_binding_available__sig: "i",
    emnapi_is_support_bigint: _emnapi_is_support_bigint,
    emnapi_is_support_bigint__deps: ["$emnapiCtx"],
    emnapi_is_support_bigint__sig: "i",
    emnapi_is_support_weakref: _emnapi_is_support_weakref,
    emnapi_is_support_weakref__deps: ["$emnapiCtx"],
    emnapi_is_support_weakref__sig: "i",
    emnapi_sync_memory: _emnapi_sync_memory,
    emnapi_sync_memory__deps: ["$emnapiCtx", "$emnapiExternalMemory", "$emnapiSyncMemory"],
    emnapi_sync_memory__sig: "ipippp",
    $emnapiTSFN: emnapiTSFN,
    $emnapiTSFN__deps: ["$emnapiCtx", "_emnapi_runtime_keepalive_pop", "$emnapiNodeBinding", "_emnapi_node_emit_async_destroy", "malloc", "free"],
    $emnapiTSFN__postset: "emnapiTSFN.init();",
    napi_acquire_threadsafe_function: _napi_acquire_threadsafe_function,
    napi_acquire_threadsafe_function__deps: ["$emnapiTSFN"],
    napi_acquire_threadsafe_function__sig: "ip",
    napi_add_env_cleanup_hook: _napi_add_env_cleanup_hook,
    napi_add_env_cleanup_hook__deps: ["$emnapiCtx"],
    napi_add_env_cleanup_hook__sig: "ippp",
    napi_adjust_external_memory: _napi_adjust_external_memory,
    napi_adjust_external_memory__deps: ["$emnapiCtx", "$emnapiSetValueI64", "emscripten_resize_heap"],
    napi_adjust_external_memory__sig: "ipjp",
    napi_call_function: _napi_call_function,
    napi_call_function__deps: ["$emnapiCtx"],
    napi_call_function__sig: "ipppppp",
    napi_call_threadsafe_function: _napi_call_threadsafe_function,
    napi_call_threadsafe_function__deps: ["$emnapiTSFN"],
    napi_call_threadsafe_function__sig: "ippi",
    $emnapiAWST: emnapiAWST,
    $emnapiAWST__deps: ["$emnapiNodeBinding", "$emnapiCtx", "$emnapiAsyncWorkPoolSize"],
    $emnapiAWST__postset: "emnapiAWST.init();",
    napi_cancel_async_work: _napi_cancel_async_work,
    napi_cancel_async_work__deps: ["$emnapiCtx", "$emnapiAWST"],
    napi_cancel_async_work__sig: "ipp",
    napi_check_object_type_tag: _napi_check_object_type_tag,
    napi_check_object_type_tag__deps: ["$emnapiCtx"],
    napi_check_object_type_tag__sig: "ipppp",
    napi_clear_last_error: _napi_clear_last_error,
    napi_clear_last_error__deps: ["$emnapiCtx"],
    napi_clear_last_error__sig: "ip",
    napi_close_callback_scope: _napi_close_callback_scope,
    napi_close_callback_scope__sig: "ipp",
    napi_close_escapable_handle_scope: _napi_close_escapable_handle_scope,
    napi_close_escapable_handle_scope__deps: ["$emnapiCtx"],
    napi_close_escapable_handle_scope__sig: "ipp",
    napi_close_handle_scope: _napi_close_handle_scope,
    napi_close_handle_scope__deps: ["$emnapiCtx"],
    napi_close_handle_scope__sig: "ipp",
    napi_coerce_to_bool: _napi_coerce_to_bool,
    napi_coerce_to_bool__deps: ["$emnapiCtx"],
    napi_coerce_to_bool__sig: "ippp",
    napi_coerce_to_number: _napi_coerce_to_number,
    napi_coerce_to_number__deps: ["$emnapiCtx"],
    napi_coerce_to_number__sig: "ippp",
    napi_coerce_to_object: _napi_coerce_to_object,
    napi_coerce_to_object__deps: ["$emnapiCtx"],
    napi_coerce_to_object__sig: "ippp",
    napi_coerce_to_string: _napi_coerce_to_string,
    napi_coerce_to_string__deps: ["$emnapiCtx"],
    napi_coerce_to_string__sig: "ippp",
    napi_create_array: _napi_create_array,
    napi_create_array__deps: ["$emnapiCtx"],
    napi_create_array__sig: "ipp",
    napi_create_array_with_length: _napi_create_array_with_length,
    napi_create_array_with_length__deps: ["$emnapiCtx"],
    napi_create_array_with_length__sig: "ippp",
    $emnapiCreateArrayBuffer: emnapiCreateArrayBuffer,
    $emnapiCreateArrayBuffer__deps: ["$emnapiExternalMemory"],
    napi_create_arraybuffer: _napi_create_arraybuffer,
    napi_create_arraybuffer__deps: ["$emnapiCtx", "$emnapiCreateArrayBuffer"],
    napi_create_arraybuffer__sig: "ipppp",
    napi_create_async_work: _napi_create_async_work,
    napi_create_async_work__deps: ["$emnapiCtx", "$emnapiAWST"],
    napi_create_async_work__sig: "ippppppp",
    napi_create_bigint_int64: _napi_create_bigint_int64,
    napi_create_bigint_int64__deps: ["$emnapiCtx"],
    napi_create_bigint_int64__sig: "ipjp",
    napi_create_bigint_uint64: _napi_create_bigint_uint64,
    napi_create_bigint_uint64__deps: ["$emnapiCtx"],
    napi_create_bigint_uint64__sig: "ipjp",
    napi_create_bigint_words: _napi_create_bigint_words,
    napi_create_bigint_words__deps: ["$emnapiCtx"],
    napi_create_bigint_words__sig: "ipippp",
    napi_create_buffer: _napi_create_buffer,
    napi_create_buffer__deps: ["$emnapiCtx", "$emnapiExternalMemory", "malloc"],
    napi_create_buffer__sig: "ippp",
    napi_create_buffer_copy: _napi_create_buffer_copy,
    napi_create_buffer_copy__deps: ["$emnapiCtx", "$emnapiCreateArrayBuffer"],
    napi_create_buffer_copy__sig: "ippppp",
    napi_create_dataview: _napi_create_dataview,
    napi_create_dataview__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_create_dataview__sig: "ippppp",
    napi_create_date: _napi_create_date,
    napi_create_date__deps: ["$emnapiCtx"],
    napi_create_date__sig: "ipdp",
    napi_create_double: _napi_create_double,
    napi_create_double__deps: ["$emnapiCtx"],
    napi_create_double__sig: "ipdp",
    napi_create_error: _napi_create_error,
    napi_create_error__deps: ["$emnapiCtx"],
    napi_create_error__sig: "ipppp",
    napi_create_external: _napi_create_external,
    napi_create_external__deps: ["$emnapiCtx"],
    napi_create_external__sig: "ippppp",
    napi_create_external_arraybuffer: _napi_create_external_arraybuffer,
    napi_create_external_arraybuffer__deps: ["$emnapiCtx", "$emnapiExternalMemory", "napi_add_finalizer"],
    napi_create_external_arraybuffer__sig: "ipppppp",
    napi_create_external_buffer: _napi_create_external_buffer,
    napi_create_external_buffer__deps: ["emnapi_create_memory_view"],
    napi_create_external_buffer__sig: "ipppppp",
    napi_create_function: _napi_create_function,
    napi_create_function__deps: ["$emnapiCtx", "_emnapi_create_function"],
    napi_create_function__sig: "ipppppp",
    napi_create_int32: _napi_create_int32,
    napi_create_int32__deps: ["$emnapiCtx"],
    napi_create_int32__sig: "ipip",
    napi_create_int64: _napi_create_int64,
    napi_create_int64__deps: ["$emnapiCtx"],
    napi_create_int64__sig: "ipjp",
    napi_create_object: _napi_create_object,
    napi_create_object__deps: ["$emnapiCtx"],
    napi_create_object__sig: "ipp",
    napi_create_promise: _napi_create_promise,
    napi_create_promise__deps: ["$emnapiCtx"],
    napi_create_promise__sig: "ippp",
    napi_create_range_error: _napi_create_range_error,
    napi_create_range_error__deps: ["$emnapiCtx"],
    napi_create_range_error__sig: "ipppp",
    napi_create_reference: _napi_create_reference,
    napi_create_reference__deps: ["$emnapiCtx"],
    napi_create_reference__sig: "ippip",
    napi_create_string_latin1: _napi_create_string_latin1,
    napi_create_string_latin1__deps: ["$emnapiString"],
    napi_create_string_latin1__sig: "ipppp",
    napi_create_string_utf16: _napi_create_string_utf16,
    napi_create_string_utf16__deps: ["$emnapiString"],
    napi_create_string_utf16__sig: "ipppp",
    napi_create_string_utf8: _napi_create_string_utf8,
    napi_create_string_utf8__deps: ["$emnapiString"],
    napi_create_string_utf8__sig: "ipppp",
    napi_create_symbol: _napi_create_symbol,
    napi_create_symbol__deps: ["$emnapiCtx"],
    napi_create_symbol__sig: "ippp",
    napi_create_threadsafe_function: _napi_create_threadsafe_function,
    napi_create_threadsafe_function__deps: ["$emnapiCtx", "$emnapiTSFN", "_emnapi_node_emit_async_init", "_emnapi_runtime_keepalive_push"],
    napi_create_threadsafe_function__sig: "ippppppppppp",
    napi_create_type_error: _napi_create_type_error,
    napi_create_type_error__deps: ["$emnapiCtx"],
    napi_create_type_error__sig: "ipppp",
    napi_create_typedarray: _napi_create_typedarray,
    napi_create_typedarray__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_create_typedarray__sig: "ipipppp",
    napi_create_uint32: _napi_create_uint32,
    napi_create_uint32__deps: ["$emnapiCtx"],
    napi_create_uint32__sig: "ipip",
    $emnapiDefineProperty: emnapiDefineProperty,
    $emnapiDefineProperty__deps: ["$emnapiCreateFunction", "$emnapiCtx"],
    napi_define_class: _napi_define_class,
    napi_define_class__deps: ["$emnapiCtx", "$emnapiCreateFunction", "$emnapiString", "$emnapiDefineProperty"],
    napi_define_class__sig: "ipppppppp",
    napi_define_properties: _napi_define_properties,
    napi_define_properties__deps: ["$emnapiCtx", "$emnapiString", "$emnapiDefineProperty"],
    napi_define_properties__sig: "ipppp",
    napi_delete_async_work: _napi_delete_async_work,
    napi_delete_async_work__deps: ["$emnapiCtx", "$emnapiAWST"],
    napi_delete_async_work__sig: "ipp",
    napi_delete_element: _napi_delete_element,
    napi_delete_element__deps: ["$emnapiCtx"],
    napi_delete_element__sig: "ippip",
    napi_delete_property: _napi_delete_property,
    napi_delete_property__deps: ["$emnapiCtx"],
    napi_delete_property__sig: "ipppp",
    napi_delete_reference: _napi_delete_reference,
    napi_delete_reference__deps: ["$emnapiCtx"],
    napi_delete_reference__sig: "ipp",
    napi_detach_arraybuffer: _napi_detach_arraybuffer,
    napi_detach_arraybuffer__deps: ["$emnapiCtx"],
    napi_detach_arraybuffer__sig: "ipp",
    napi_escape_handle: _napi_escape_handle,
    napi_escape_handle__deps: ["$emnapiCtx"],
    napi_escape_handle__sig: "ipppp",
    napi_fatal_error: _napi_fatal_error,
    napi_fatal_error__deps: ["$emnapiString", "$emnapiNodeBinding"],
    napi_fatal_error__sig: "vpppp",
    napi_fatal_exception: _napi_fatal_exception,
    napi_fatal_exception__deps: ["$emnapiCtx"],
    napi_fatal_exception__sig: "ipp",
    napi_get_all_property_names: _napi_get_all_property_names,
    napi_get_all_property_names__deps: ["$emnapiCtx"],
    napi_get_all_property_names__sig: "ippiiip",
    napi_get_and_clear_last_exception: _napi_get_and_clear_last_exception,
    napi_get_and_clear_last_exception__deps: ["$emnapiCtx"],
    napi_get_and_clear_last_exception__sig: "ipp",
    napi_get_array_length: _napi_get_array_length,
    napi_get_array_length__deps: ["$emnapiCtx"],
    napi_get_array_length__sig: "ippp",
    napi_get_arraybuffer_info: _napi_get_arraybuffer_info,
    napi_get_arraybuffer_info__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_get_arraybuffer_info__sig: "ipppp",
    napi_get_boolean: _napi_get_boolean,
    napi_get_boolean__deps: ["$emnapiCtx"],
    napi_get_boolean__sig: "ipip",
    napi_get_dataview_info: _napi_get_dataview_info,
    napi_get_dataview_info__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_get_dataview_info__sig: "ipppppp",
    napi_get_typedarray_info: _napi_get_typedarray_info,
    napi_get_typedarray_info__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    napi_get_typedarray_info__sig: "ippppppp",
    napi_get_buffer_info: _napi_get_buffer_info,
    napi_get_buffer_info__deps: ["$emnapiCtx", "napi_get_dataview_info", "napi_get_typedarray_info"],
    napi_get_buffer_info__sig: "ipppp",
    napi_get_cb_info: _napi_get_cb_info,
    napi_get_cb_info__deps: ["$emnapiCtx"],
    napi_get_cb_info__sig: "ipppppp",
    napi_get_date_value: _napi_get_date_value,
    napi_get_date_value__deps: ["$emnapiCtx"],
    napi_get_date_value__sig: "ippp",
    napi_get_element: _napi_get_element,
    napi_get_element__deps: ["$emnapiCtx"],
    napi_get_element__sig: "ippip",
    napi_get_global: _napi_get_global,
    napi_get_global__deps: ["$emnapiCtx"],
    napi_get_global__sig: "ipp",
    napi_get_instance_data: _napi_get_instance_data,
    napi_get_instance_data__deps: ["$emnapiCtx"],
    napi_get_instance_data__sig: "ipp",
    napi_get_named_property: _napi_get_named_property,
    napi_get_named_property__deps: ["$emnapiCtx", "$emnapiString"],
    napi_get_named_property__sig: "ipppp",
    napi_get_new_target: _napi_get_new_target,
    napi_get_new_target__deps: ["$emnapiCtx"],
    napi_get_new_target__sig: "ippp",
    napi_get_null: _napi_get_null,
    napi_get_null__deps: ["$emnapiCtx"],
    napi_get_null__sig: "ipp",
    napi_get_property: _napi_get_property,
    napi_get_property__deps: ["$emnapiCtx"],
    napi_get_property__sig: "ipppp",
    napi_get_property_names: _napi_get_property_names,
    napi_get_property_names__deps: ["napi_get_all_property_names"],
    napi_get_property_names__sig: "ippp",
    napi_get_prototype: _napi_get_prototype,
    napi_get_prototype__deps: ["$emnapiCtx"],
    napi_get_prototype__sig: "ippp",
    napi_get_reference_value: _napi_get_reference_value,
    napi_get_reference_value__deps: ["$emnapiCtx"],
    napi_get_reference_value__sig: "ippp",
    napi_get_threadsafe_function_context: _napi_get_threadsafe_function_context,
    napi_get_threadsafe_function_context__deps: ["$emnapiTSFN"],
    napi_get_threadsafe_function_context__sig: "ipp",
    napi_get_undefined: _napi_get_undefined,
    napi_get_undefined__deps: ["$emnapiCtx"],
    napi_get_undefined__sig: "ipp",
    napi_get_value_bigint_int64: _napi_get_value_bigint_int64,
    napi_get_value_bigint_int64__deps: ["$emnapiCtx"],
    napi_get_value_bigint_int64__sig: "ipppp",
    napi_get_value_bigint_uint64: _napi_get_value_bigint_uint64,
    napi_get_value_bigint_uint64__deps: ["$emnapiCtx"],
    napi_get_value_bigint_uint64__sig: "ipppp",
    napi_get_value_bigint_words: _napi_get_value_bigint_words,
    napi_get_value_bigint_words__deps: ["$emnapiCtx"],
    napi_get_value_bigint_words__sig: "ippppp",
    napi_get_value_bool: _napi_get_value_bool,
    napi_get_value_bool__deps: ["$emnapiCtx"],
    napi_get_value_bool__sig: "ippp",
    napi_get_value_double: _napi_get_value_double,
    napi_get_value_double__deps: ["$emnapiCtx"],
    napi_get_value_double__sig: "ippp",
    napi_get_value_external: _napi_get_value_external,
    napi_get_value_external__deps: ["$emnapiCtx"],
    napi_get_value_external__sig: "ippp",
    napi_get_value_int32: _napi_get_value_int32,
    napi_get_value_int32__deps: ["$emnapiCtx"],
    napi_get_value_int32__sig: "ippp",
    napi_get_value_int64: _napi_get_value_int64,
    napi_get_value_int64__deps: ["$emnapiCtx", "$emnapiSetValueI64"],
    napi_get_value_int64__sig: "ippp",
    napi_get_value_string_latin1: _napi_get_value_string_latin1,
    napi_get_value_string_latin1__deps: ["$emnapiCtx"],
    napi_get_value_string_latin1__sig: "ippppp",
    napi_get_value_string_utf16: _napi_get_value_string_utf16,
    napi_get_value_string_utf16__deps: ["$emnapiCtx", "$emnapiString"],
    napi_get_value_string_utf16__sig: "ippppp",
    napi_get_value_string_utf8: _napi_get_value_string_utf8,
    napi_get_value_string_utf8__deps: ["$emnapiCtx", "$emnapiString"],
    napi_get_value_string_utf8__sig: "ippppp",
    napi_get_value_uint32: _napi_get_value_uint32,
    napi_get_value_uint32__deps: ["$emnapiCtx"],
    napi_get_value_uint32__sig: "ippp",
    napi_get_version: _napi_get_version,
    napi_get_version__deps: ["$emnapiCtx"],
    napi_get_version__sig: "ipp",
    napi_has_element: _napi_has_element,
    napi_has_element__deps: ["$emnapiCtx"],
    napi_has_element__sig: "ippip",
    napi_has_named_property: _napi_has_named_property,
    napi_has_named_property__deps: ["$emnapiCtx", "$emnapiString"],
    napi_has_named_property__sig: "ipppp",
    napi_has_own_property: _napi_has_own_property,
    napi_has_own_property__deps: ["$emnapiCtx"],
    napi_has_own_property__sig: "ipppp",
    napi_has_property: _napi_has_property,
    napi_has_property__deps: ["$emnapiCtx"],
    napi_has_property__sig: "ipppp",
    napi_instanceof: _napi_instanceof,
    napi_instanceof__deps: ["$emnapiCtx"],
    napi_instanceof__sig: "ipppp",
    napi_is_array: _napi_is_array,
    napi_is_array__deps: ["$emnapiCtx"],
    napi_is_array__sig: "ippp",
    napi_is_arraybuffer: _napi_is_arraybuffer,
    napi_is_arraybuffer__deps: ["$emnapiCtx"],
    napi_is_arraybuffer__sig: "ippp",
    napi_is_buffer: _napi_is_buffer,
    napi_is_buffer__deps: ["$emnapiCtx"],
    napi_is_buffer__sig: "ippp",
    napi_is_dataview: _napi_is_dataview,
    napi_is_dataview__deps: ["$emnapiCtx"],
    napi_is_dataview__sig: "ippp",
    napi_is_date: _napi_is_date,
    napi_is_date__deps: ["$emnapiCtx"],
    napi_is_date__sig: "ippp",
    napi_is_detached_arraybuffer: _napi_is_detached_arraybuffer,
    napi_is_detached_arraybuffer__deps: ["$emnapiCtx"],
    napi_is_detached_arraybuffer__sig: "ippp",
    napi_is_error: _napi_is_error,
    napi_is_error__deps: ["$emnapiCtx"],
    napi_is_error__sig: "ippp",
    napi_is_exception_pending: _napi_is_exception_pending,
    napi_is_exception_pending__deps: ["$emnapiCtx"],
    napi_is_exception_pending__sig: "ipp",
    napi_is_promise: _napi_is_promise,
    napi_is_promise__deps: ["$emnapiCtx"],
    napi_is_promise__sig: "ippp",
    napi_is_typedarray: _napi_is_typedarray,
    napi_is_typedarray__deps: ["$emnapiCtx"],
    napi_is_typedarray__sig: "ippp",
    napi_make_callback: _napi_make_callback,
    napi_make_callback__deps: ["$emnapiCtx", "$emnapiNodeBinding"],
    napi_make_callback__sig: "ippppppp",
    napi_new_instance: _napi_new_instance,
    napi_new_instance__deps: ["$emnapiCtx"],
    napi_new_instance__sig: "ippppp",
    napi_object_freeze: _napi_object_freeze,
    napi_object_freeze__deps: ["$emnapiCtx"],
    napi_object_freeze__sig: "ipp",
    napi_object_seal: _napi_object_seal,
    napi_object_seal__deps: ["$emnapiCtx"],
    napi_object_seal__sig: "ipp",
    napi_open_callback_scope: _napi_open_callback_scope,
    napi_open_callback_scope__sig: "ipppp",
    napi_open_escapable_handle_scope: _napi_open_escapable_handle_scope,
    napi_open_escapable_handle_scope__deps: ["$emnapiCtx"],
    napi_open_escapable_handle_scope__sig: "ipp",
    napi_open_handle_scope: _napi_open_handle_scope,
    napi_open_handle_scope__deps: ["$emnapiCtx"],
    napi_open_handle_scope__sig: "ipp",
    napi_queue_async_work: _napi_queue_async_work,
    napi_queue_async_work__deps: ["$emnapiCtx", "$emnapiAWST"],
    napi_queue_async_work__sig: "ipp",
    napi_ref_threadsafe_function: _napi_ref_threadsafe_function,
    napi_ref_threadsafe_function__deps: ["$emnapiTSFN", "_emnapi_runtime_keepalive_push", "$emnapiCtx"],
    napi_ref_threadsafe_function__sig: "ipp",
    napi_reference_ref: _napi_reference_ref,
    napi_reference_ref__deps: ["$emnapiCtx"],
    napi_reference_ref__sig: "ippp",
    napi_reference_unref: _napi_reference_unref,
    napi_reference_unref__deps: ["$emnapiCtx"],
    napi_reference_unref__sig: "ippp",
    napi_reject_deferred: _napi_reject_deferred,
    napi_reject_deferred__deps: ["$emnapiCtx"],
    napi_reject_deferred__sig: "ippp",
    napi_release_threadsafe_function: _napi_release_threadsafe_function,
    napi_release_threadsafe_function__deps: ["$emnapiTSFN"],
    napi_release_threadsafe_function__sig: "ipi",
    napi_remove_env_cleanup_hook: _napi_remove_env_cleanup_hook,
    napi_remove_env_cleanup_hook__deps: ["$emnapiCtx"],
    napi_remove_env_cleanup_hook__sig: "ippp",
    $emnapiUnwrap: emnapiUnwrap,
    $emnapiUnwrap__deps: ["$emnapiCtx"],
    napi_remove_wrap: _napi_remove_wrap,
    napi_remove_wrap__deps: ["$emnapiUnwrap"],
    napi_remove_wrap__sig: "ippp",
    napi_resolve_deferred: _napi_resolve_deferred,
    napi_resolve_deferred__deps: ["$emnapiCtx"],
    napi_resolve_deferred__sig: "ippp",
    napi_set_last_error: _napi_set_last_error,
    napi_set_last_error__deps: ["$emnapiCtx"],
    napi_set_last_error__sig: "ipiip",
    napi_run_script: _napi_run_script,
    napi_run_script__deps: ["$emnapiCtx", "napi_set_last_error"],
    napi_run_script__sig: "ippp",
    napi_set_element: _napi_set_element,
    napi_set_element__deps: ["$emnapiCtx"],
    napi_set_element__sig: "ippip",
    napi_set_instance_data: _napi_set_instance_data,
    napi_set_instance_data__deps: ["$emnapiCtx"],
    napi_set_instance_data__sig: "ipppp",
    napi_set_named_property: _napi_set_named_property,
    napi_set_named_property__deps: ["$emnapiCtx", "$emnapiString"],
    napi_set_named_property__sig: "ipppp",
    napi_set_property: _napi_set_property,
    napi_set_property__deps: ["$emnapiCtx"],
    napi_set_property__sig: "ipppp",
    napi_strict_equals: _napi_strict_equals,
    napi_strict_equals__deps: ["$emnapiCtx"],
    napi_strict_equals__sig: "ipppp",
    napi_throw: _napi_throw,
    napi_throw__deps: ["$emnapiCtx"],
    napi_throw__sig: "ipp",
    napi_throw_error: _napi_throw_error,
    napi_throw_error__deps: ["$emnapiCtx", "$emnapiString"],
    napi_throw_error__sig: "ippp",
    napi_throw_range_error: _napi_throw_range_error,
    napi_throw_range_error__deps: ["$emnapiCtx", "$emnapiString"],
    napi_throw_range_error__sig: "ippp",
    napi_throw_type_error: _napi_throw_type_error,
    napi_throw_type_error__deps: ["$emnapiCtx", "$emnapiString"],
    napi_throw_type_error__sig: "ippp",
    napi_type_tag_object: _napi_type_tag_object,
    napi_type_tag_object__deps: ["$emnapiCtx"],
    napi_type_tag_object__sig: "ippp",
    napi_typeof: _napi_typeof,
    napi_typeof__deps: ["$emnapiCtx"],
    napi_typeof__sig: "ippp",
    napi_unref_threadsafe_function: _napi_unref_threadsafe_function,
    napi_unref_threadsafe_function__deps: ["$emnapiTSFN", "_emnapi_runtime_keepalive_pop", "$emnapiCtx"],
    napi_unref_threadsafe_function__sig: "ipp",
    napi_unwrap: _napi_unwrap,
    napi_unwrap__deps: ["$emnapiUnwrap"],
    napi_unwrap__sig: "ippp",
    $emnapiWrap: emnapiWrap,
    $emnapiWrap__deps: ["$emnapiCtx", "$emnapiGetHandle"],
    napi_wrap: _napi_wrap,
    napi_wrap__deps: ["$emnapiWrap"],
    napi_wrap__sig: "ipppppp",
    node_api_create_buffer_from_arraybuffer: _node_api_create_buffer_from_arraybuffer,
    node_api_create_buffer_from_arraybuffer__deps: ["$emnapiCtx", "$emnapiExternalMemory"],
    node_api_create_buffer_from_arraybuffer__sig: "ippppp",
    node_api_create_external_string_latin1: _node_api_create_external_string_latin1,
    node_api_create_external_string_latin1__deps: ["$emnapiString", "napi_create_string_latin1"],
    node_api_create_external_string_latin1__sig: "ippppppp",
    node_api_create_external_string_utf16: _node_api_create_external_string_utf16,
    node_api_create_external_string_utf16__deps: ["$emnapiString", "napi_create_string_utf16"],
    node_api_create_external_string_utf16__sig: "ippppppp",
    node_api_create_object_with_properties: _node_api_create_object_with_properties,
    node_api_create_object_with_properties__deps: ["$emnapiCtx"],
    node_api_create_object_with_properties__sig: "ipppppp",
    node_api_create_property_key_latin1: _node_api_create_property_key_latin1,
    node_api_create_property_key_latin1__deps: ["napi_create_string_latin1"],
    node_api_create_property_key_latin1__sig: "ipppp",
    node_api_create_property_key_utf16: _node_api_create_property_key_utf16,
    node_api_create_property_key_utf16__deps: ["napi_create_string_utf16"],
    node_api_create_property_key_utf16__sig: "ipppp",
    node_api_create_property_key_utf8: _node_api_create_property_key_utf8,
    node_api_create_property_key_utf8__deps: ["napi_create_string_utf8"],
    node_api_create_property_key_utf8__sig: "ipppp",
    node_api_create_sharedarraybuffer: _node_api_create_sharedarraybuffer,
    node_api_create_sharedarraybuffer__deps: ["$emnapiCtx", "$emnapiCreateArrayBuffer"],
    node_api_create_sharedarraybuffer__sig: "ipppp",
    node_api_create_syntax_error: _node_api_create_syntax_error,
    node_api_create_syntax_error__deps: ["$emnapiCtx"],
    node_api_create_syntax_error__sig: "ipppp",
    node_api_is_sharedarraybuffer: _node_api_is_sharedarraybuffer,
    node_api_is_sharedarraybuffer__deps: ["$emnapiCtx"],
    node_api_is_sharedarraybuffer__sig: "ippp",
    node_api_post_finalizer: _node_api_post_finalizer,
    node_api_post_finalizer__deps: ["$emnapiCtx"],
    node_api_post_finalizer__sig: "ipppp",
    node_api_set_prototype: _node_api_set_prototype,
    node_api_set_prototype__deps: ["$emnapiCtx"],
    node_api_set_prototype__sig: "ippp",
    node_api_symbol_for: _node_api_symbol_for,
    node_api_symbol_for__deps: ["$emnapiCtx", "$emnapiString"],
    node_api_symbol_for__sig: "ipppp",
    node_api_throw_syntax_error: _node_api_throw_syntax_error,
    node_api_throw_syntax_error__deps: ["$emnapiCtx", "$emnapiString"],
    node_api_throw_syntax_error__sig: "ippp"
});
