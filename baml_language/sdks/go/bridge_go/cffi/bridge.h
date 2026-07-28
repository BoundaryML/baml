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
typedef void (*CallFunctionFn)(const uint8_t *encoded_args, size_t length, uint32_t id);
typedef void (*CallbackFn)(uint32_t call_id, const int8_t *content, size_t length);
typedef void (*RegisterCallbackFn)(CallbackFn cb);
typedef uint32_t BamlCffiStatus;
typedef BamlCffiStatus (*BamlHandleCloneFn)(uint64_t key, uint64_t *out_key);
typedef BamlCffiStatus (*BamlHandleReleaseFn)(uint64_t key);
typedef BamlCffiStatus (*TestonlySeedFunctionRefFn)(uint64_t global_index, uint64_t *out_key, int32_t *out_handle_type);
typedef int32_t (*CancelFunctionCallFn)(uint32_t id);
typedef void (*FlushEventsFn)(void);
// Host-value callable support
typedef void (*HostDispatchFn)(uint64_t host_value_key, uint32_t call_id, const uint8_t *args, size_t length);
typedef void (*HostReleaseFn)(uint64_t host_value_key);
typedef void (*RegisterHostDispatchCallbackFn)(HostDispatchFn cb);
typedef void (*RegisterHostReleaseCallbackFn)(HostReleaseFn cb);
typedef void (*CompleteHostCallFn)(uint32_t call_id, int32_t is_error, const int8_t *content, size_t length);

// Static function pointers stored as void* to avoid type ambiguity
static void *versionFnPtr = NULL;
static void *createBamlRuntimeFnPtr = NULL;
static void *destroyBamlRuntimeFnPtr = NULL;
static void *freeBufferFnPtr = NULL;
static void *callFunctionFnPtr = NULL;
static void *registerCallbackFnPtr = NULL;
static void *bamlHandleCloneFnPtr = NULL;
static void *bamlHandleReleaseFnPtr = NULL;
static void *testonlySeedFunctionRefFnPtr = NULL;
static void *cancelFunctionCallFnPtr = NULL;
static void *flushEventsFnPtr = NULL;
// Host-value callable function pointers
static void *registerHostDispatchCallbackFnPtr = NULL;
static void *registerHostReleaseCallbackFnPtr = NULL;
static void *completeHostCallFnPtr = NULL;

// Setters
static void setVersionFn(void *fn) { versionFnPtr = fn; }
static void setCreateBamlRuntimeFn(void *fn) { createBamlRuntimeFnPtr = fn; }
static void setDestroyBamlRuntimeFn(void *fn) { destroyBamlRuntimeFnPtr = fn; }
static void setFreeBufferFn(void *fn) { freeBufferFnPtr = fn; }
static void setCallFunctionFn(void *fn) { callFunctionFnPtr = fn; }
static void setRegisterCallbackFn(void *fn) { registerCallbackFnPtr = fn; }
static void setBamlHandleCloneFn(void *fn) { bamlHandleCloneFnPtr = fn; }
static void setBamlHandleReleaseFn(void *fn) { bamlHandleReleaseFnPtr = fn; }
static void setTestonlySeedFunctionRefFn(void *fn) { testonlySeedFunctionRefFnPtr = fn; }
static void setCancelFunctionCallFn(void *fn) { cancelFunctionCallFnPtr = fn; }
static void setFlushEventsFn(void *fn) { flushEventsFnPtr = fn; }
// Host-value callable setters
static void setRegisterHostDispatchCallbackFn(void *fn) { registerHostDispatchCallbackFnPtr = fn; }
static void setRegisterHostReleaseCallbackFn(void *fn) { registerHostReleaseCallbackFnPtr = fn; }
static void setCompleteHostCallFn(void *fn) { completeHostCallFnPtr = fn; }

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
static void wrapCallFunction(const uint8_t *encoded_args, size_t length, uint32_t id) {
    if (callFunctionFnPtr) ((CallFunctionFn)callFunctionFnPtr)(encoded_args, length, id);
}
static void wrapRegisterCallback(CallbackFn cb) {
    if (registerCallbackFnPtr) ((RegisterCallbackFn)registerCallbackFnPtr)(cb);
}
static BamlCffiStatus wrapBamlHandleClone(uint64_t key, uint64_t *out_key) {
    if (bamlHandleCloneFnPtr) return ((BamlHandleCloneFn)bamlHandleCloneFnPtr)(key, out_key);
    return 4;
}
static BamlCffiStatus wrapBamlHandleRelease(uint64_t key) {
    if (bamlHandleReleaseFnPtr) return ((BamlHandleReleaseFn)bamlHandleReleaseFnPtr)(key);
    return 4;
}
static BamlCffiStatus wrapTestonlySeedFunctionRef(uint64_t global_index, uint64_t *out_key, int32_t *out_handle_type) {
    if (testonlySeedFunctionRefFnPtr) return ((TestonlySeedFunctionRefFn)testonlySeedFunctionRefFnPtr)(global_index, out_key, out_handle_type);
    return 4;
}
static int32_t wrapCancelFunctionCall(uint32_t id) {
    if (cancelFunctionCallFnPtr) return ((CancelFunctionCallFn)cancelFunctionCallFnPtr)(id);
    return 1;
}
static void wrapFlushEvents(void) {
    if (flushEventsFnPtr) ((FlushEventsFn)flushEventsFnPtr)();
}
// Host-value callable wrappers
static void wrapRegisterHostDispatchCallback(HostDispatchFn cb) {
    if (registerHostDispatchCallbackFnPtr)
        ((RegisterHostDispatchCallbackFn)registerHostDispatchCallbackFnPtr)(cb);
}
static void wrapRegisterHostReleaseCallback(HostReleaseFn cb) {
    if (registerHostReleaseCallbackFnPtr)
        ((RegisterHostReleaseCallbackFn)registerHostReleaseCallbackFnPtr)(cb);
}
static void wrapCompleteHostCall(uint32_t call_id, int32_t is_error, const int8_t *content, size_t length) {
    if (completeHostCallFnPtr)
        ((CompleteHostCallFn)completeHostCallFnPtr)(call_id, is_error, content, length);
}

#endif // BRIDGE_GO_H
