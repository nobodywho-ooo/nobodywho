#ifndef EMNAPI_INCLUDE_COMMON_H_
#define EMNAPI_INCLUDE_COMMON_H_

#ifdef __EMSCRIPTEN__
#define EMNAPI_EXTERN __attribute__((__import_module__("env")))
#else
#define EMNAPI_EXTERN __attribute__((__import_module__("emnapi")))
#endif

#define EMNAPI_INTERNAL_EXTERN __attribute__((__import_module__("env")))

#ifdef __cplusplus
#ifndef EXTERN_C_START
#define EXTERN_C_START extern "C" {
#endif

#ifndef EXTERN_C_END
#define EXTERN_C_END }
#endif
#else
#ifndef EXTERN_C_START
#define EXTERN_C_START
#endif

#ifndef EXTERN_C_END
#define EXTERN_C_END
#endif
#endif

#endif
