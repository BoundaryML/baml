#ifndef BRIDGE_GO_H
#define BRIDGE_GO_H

#include <dlfcn.h>
#include <stdint.h>
#include <stdlib.h>

// Function pointer types
typedef struct { const int8_t *ptr; size_t len; } Buffer;
typedef Buffer (*VersionFn)(void);
typedef void* (*CreateBamlRuntimeFn)(const char *root_path, const char *src_files_json);
typedef void (*DestroyBamlRuntimeFn)(const void *runtime);
typedef void (*FreeBufferFn)(Buffer buf);
typedef void (*CallFunctionFn)(const char *function_name, const uint8_t *encoded_args, size_t length, uint32_t id);
typedef void (*CallbackFn)(uint32_t call_id, int32_t is_error, const int8_t *content, size_t length);
typedef void (*RegisterCallbackFn)(CallbackFn cb);
typedef uint64_t (*CloneHandleFn)(uint64_t key);
typedef void (*ReleaseHandleFn)(uint64_t key);
typedef int32_t (*CancelFunctionCallFn)(uint32_t id);
typedef void (*FlushEventsFn)(void);

// Static function pointers stored as void* to avoid type ambiguity
static void *versionFnPtr = NULL;
static void *createBamlRuntimeFnPtr = NULL;
static void *destroyBamlRuntimeFnPtr = NULL;
static void *freeBufferFnPtr = NULL;
static void *callFunctionFnPtr = NULL;
static void *registerCallbackFnPtr = NULL;
static void *cloneHandleFnPtr = NULL;
static void *releaseHandleFnPtr = NULL;
static void *cancelFunctionCallFnPtr = NULL;
static void *flushEventsFnPtr = NULL;

// Setters
static void setVersionFn(void *fn) { versionFnPtr = fn; }
static void setCreateBamlRuntimeFn(void *fn) { createBamlRuntimeFnPtr = fn; }
static void setDestroyBamlRuntimeFn(void *fn) { destroyBamlRuntimeFnPtr = fn; }
static void setFreeBufferFn(void *fn) { freeBufferFnPtr = fn; }
static void setCallFunctionFn(void *fn) { callFunctionFnPtr = fn; }
static void setRegisterCallbackFn(void *fn) { registerCallbackFnPtr = fn; }
static void setCloneHandleFn(void *fn) { cloneHandleFnPtr = fn; }
static void setReleaseHandleFn(void *fn) { releaseHandleFnPtr = fn; }
static void setCancelFunctionCallFn(void *fn) { cancelFunctionCallFnPtr = fn; }
static void setFlushEventsFn(void *fn) { flushEventsFnPtr = fn; }

// Wrappers — cast the void* to the correct function pointer type at call time
static Buffer wrapVersion(void) {
    if (versionFnPtr) return ((VersionFn)versionFnPtr)();
    return (Buffer){NULL, 0};
}
static void* wrapCreateBamlRuntime(const char *root_path, const char *src_files_json) {
    if (createBamlRuntimeFnPtr) return ((CreateBamlRuntimeFn)createBamlRuntimeFnPtr)(root_path, src_files_json);
    return NULL;
}
static void wrapDestroyBamlRuntime(const void *runtime) {
    if (destroyBamlRuntimeFnPtr) ((DestroyBamlRuntimeFn)destroyBamlRuntimeFnPtr)(runtime);
}
static void wrapFreeBuffer(const int8_t *ptr, size_t len) {
    if (freeBufferFnPtr) {
        Buffer buf = {ptr, len};
        ((FreeBufferFn)freeBufferFnPtr)(buf);
    }
}
static void wrapCallFunction(const char *function_name, const uint8_t *encoded_args, size_t length, uint32_t id) {
    if (callFunctionFnPtr) ((CallFunctionFn)callFunctionFnPtr)(function_name, encoded_args, length, id);
}
static void wrapRegisterCallback(CallbackFn cb) {
    if (registerCallbackFnPtr) ((RegisterCallbackFn)registerCallbackFnPtr)(cb);
}
static uint64_t wrapCloneHandle(uint64_t key) {
    if (cloneHandleFnPtr) return ((CloneHandleFn)cloneHandleFnPtr)(key);
    return 0;
}
static void wrapReleaseHandle(uint64_t key) {
    if (releaseHandleFnPtr) ((ReleaseHandleFn)releaseHandleFnPtr)(key);
}
static int32_t wrapCancelFunctionCall(uint32_t id) {
    if (cancelFunctionCallFnPtr) return ((CancelFunctionCallFn)cancelFunctionCallFnPtr)(id);
    return 1;
}
static void wrapFlushEvents(void) {
    if (flushEventsFnPtr) ((FlushEventsFn)flushEventsFnPtr)();
}

#endif // BRIDGE_GO_H
