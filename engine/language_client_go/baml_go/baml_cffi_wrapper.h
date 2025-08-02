#pragma once

#ifndef BAML_CFFI_WRAPPER_H
#define BAML_CFFI_WRAPPER_H

#include "../pkg/cffi/baml_cffi_generated.h"

// Function declarations for the wrapper functions

void SetVersionFn(void *fn);
const char *WrapVersion();

void SetCreateBamlRuntimeFn(void *fn);
const void *WrapCreateBamlRuntime(const char *root_path, const char *src_files_json, const char *env_vars_json);

void SetDestroyBamlRuntimeFn(void *fn);
void WrapDestroyBamlRuntime(const void *runtime);

void SetInvokeRuntimeCliFn(void *fn);
int WrapInvokeRuntimeCli(const char *const *args);

void SetRegisterCallbacksFn(void *fn);
void WrapRegisterCallbacks(CallbackFn callback_fn, CallbackFn error_callback_fn, OnTickCallbackFn on_tick_callback_fn);

void SetCallFunctionFromCFn(void *fn);
const void *WrapCallFunctionFromC(const void *runtime, const char *function_name, const char *encoded_args, uintptr_t length, uint32_t id);

void SetCallFunctionStreamFromCFn(void *fn);
const void *WrapCallFunctionStreamFromC(const void *runtime, const char *function_name, const char *encoded_args, uintptr_t length, uint32_t id);

void SetCallObjectConstructorFn(void *fn);
Buffer WrapCallObjectConstructor(const char *encoded_args, uintptr_t length);

void SetCallObjectMethodFunctionFn(void *fn);
Buffer WrapCallObjectMethodFunction(const void *runtime, const char *encoded_args, uintptr_t length);

void SetFreeBufferFn(void *fn);
void WrapFreeBuffer(Buffer buffer);


#endif // BAML_CFFI_WRAPPER_H
