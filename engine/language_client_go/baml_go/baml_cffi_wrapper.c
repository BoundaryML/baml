#include "baml_cffi_wrapper.h"

// Define a macro to simplify the creation of function pointers and wrappers
#define DEFINE_WRAPPER_FUNCTIONS(ReturnType, FunctionName, ParameterTypes, Parameters, DefaultReturnValue) \
    typedef ReturnType (*Type##FunctionName) ParameterTypes; \
    static Type##FunctionName FunctionName##Fn = NULL; \
    void Set##FunctionName##Fn(void *fn) { \
        FunctionName##Fn = (Type##FunctionName)fn; \
    } \
    ReturnType Wrap##FunctionName ParameterTypes { \
        if (FunctionName##Fn) { \
            return FunctionName##Fn Parameters; \
        } \
        return DefaultReturnValue; \
    }

static Buffer zero_buffer = {NULL, 0};

// Use the macro to define the functions
DEFINE_WRAPPER_FUNCTIONS(const char *, Version, (), (), NULL)
DEFINE_WRAPPER_FUNCTIONS(const void *, CreateBamlRuntime, (const char *root_path, const char *src_files_json, const char *env_vars_json), (root_path, src_files_json, env_vars_json), NULL)
DEFINE_WRAPPER_FUNCTIONS(void, DestroyBamlRuntime, (const void *runtime), (runtime), )
DEFINE_WRAPPER_FUNCTIONS(int, InvokeRuntimeCli, (const char *const *args), (args), -1)
DEFINE_WRAPPER_FUNCTIONS(void, RegisterCallbacks, (CallbackFn callback_fn, CallbackFn error_callback_fn, OnTickCallbackFn on_tick_callback_fn), (callback_fn, error_callback_fn, on_tick_callback_fn), )
DEFINE_WRAPPER_FUNCTIONS(const void *, CallFunctionFromC, (const void *runtime, const char *function_name, const char *encoded_args, uintptr_t length, uint32_t id), (runtime, function_name, encoded_args, length, id), NULL)
DEFINE_WRAPPER_FUNCTIONS(const void *, CallFunctionStreamFromC, (const void *runtime, const char *function_name, const char *encoded_args, uintptr_t length, uint32_t id), (runtime, function_name, encoded_args, length, id), NULL)
DEFINE_WRAPPER_FUNCTIONS(Buffer, CallObjectMethodFunction, (const void *runtime, const char *encoded_args, uintptr_t length), (runtime, encoded_args, length), zero_buffer)
DEFINE_WRAPPER_FUNCTIONS(Buffer, CallObjectConstructor, (const char *encoded_args, uintptr_t length), (encoded_args, length), zero_buffer)
DEFINE_WRAPPER_FUNCTIONS(void, FreeBuffer, (Buffer buffer), (buffer), )