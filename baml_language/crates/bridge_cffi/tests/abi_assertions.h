#ifndef BAML_CFFI_TEST_ABI_ASSERTIONS_H
#define BAML_CFFI_TEST_ABI_ASSERTIONS_H

#include "baml_cffi.h"

#include <limits.h>

#if defined(__cplusplus)
#  include <type_traits>
#  define BAML_STATIC_ASSERT(condition, message) static_assert(condition, message)
#  define BAML_ALIGNOF(type) alignof(type)
#  define BAML_ASSERT_FIELD_TYPE(field, type) \
    static_assert(std::is_same<decltype(BamlApiV1::field), type>::value, #field " type drifted");
#else
#  define BAML_STATIC_ASSERT(condition, message) _Static_assert(condition, message)
#  define BAML_ALIGNOF(type) _Alignof(type)
/* Assigning each field to its named function-pointer type makes strict-warning
 * C compilers reject calling-convention, parameter, or return-type drift. */
#  define BAML_ASSERT_FIELD_TYPE(field, type)                             \
    static inline void baml_assert_##field##_type(const BamlApiV1 *api) { \
      type value = api->field;                                            \
      (void)value;                                                        \
    }
#endif

#define BAML_ASSERT_AFTER(previous, field)                                    \
  BAML_STATIC_ASSERT(                                                         \
      offsetof(BamlApiV1, field) >=                                           \
          offsetof(BamlApiV1, previous) + sizeof(((BamlApiV1 *)0)->previous), \
      #field " must follow " #previous)

BAML_STATIC_ASSERT(CHAR_BIT == 8, "the BAML ABI requires 8-bit bytes");
BAML_STATIC_ASSERT(sizeof(uint32_t) == 4, "uint32_t must be 32 bits");
BAML_STATIC_ASSERT(sizeof(int32_t) == 4, "int32_t must be 32 bits");
BAML_STATIC_ASSERT(sizeof(uint64_t) == 8, "uint64_t must be 64 bits");
BAML_STATIC_ASSERT(sizeof(size_t) == sizeof(void *), "size_t must match Rust usize");
BAML_STATIC_ASSERT(sizeof(BamlCffiStatus) == sizeof(uint32_t), "status width drifted");
BAML_STATIC_ASSERT(sizeof(BamlBridgeLanguage) == sizeof(uint32_t), "language width drifted");
BAML_STATIC_ASSERT(sizeof(BamlCffiMediaKind) == sizeof(int32_t), "media-kind width drifted");
BAML_STATIC_ASSERT(sizeof(BamlCffiHandleType) == sizeof(int32_t), "handle-type width drifted");

BAML_STATIC_ASSERT(BAML_CFFI_STATUS_OK == 0, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_STATUS_INVALID_HANDLE == 1, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_STATUS_TYPE_MISMATCH == 2, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_STATUS_UNSUPPORTED_HANDLE_TYPE == 3, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_STATUS_INTERNAL_ERROR == 4, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_STATUS_UNEXPECTED_NULLPTR == 5, "status discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_NODE_JS == 1, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_PYTHON == 2, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_GO == 3, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_RUST == 4, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_C_SHARP == 5, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_CPP == 6, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_JAVA == 7, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_SWIFT == 8, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_WEB == 9, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_BRIDGE_LANGUAGE_RUBY == 10, "language discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_UNSPECIFIED == 0, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_IMAGE == 1, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_AUDIO == 2, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_PDF == 3, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_VIDEO == 4, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_MEDIA_KIND_GENERIC == 5, "media discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_UNSPECIFIED == 0, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_UNTAGGED_RUST_DATA == 1, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_UNTAGGED_BEX_HEAP == 2, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_FUNCTION_REF == 5, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_MEDIA_IMAGE == 6, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_MEDIA_AUDIO == 7, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_MEDIA_VIDEO == 8, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_MEDIA_PDF == 9, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_MEDIA_GENERIC == 10, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_PROMPT_AST == 11, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_COLLECTOR == 12, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_TYPE == 13, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_TAGGED_HEAP_HANDLE == 14, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_HOST_VALUE_CALLABLE == 15, "handle discriminant drifted");
BAML_STATIC_ASSERT(BAML_CFFI_HANDLE_TYPE_HOST_VALUE_OPAQUE == 16, "handle discriminant drifted");

BAML_STATIC_ASSERT(offsetof(BamlBuffer, ptr) == 0, "buffer pointer must be first");
BAML_STATIC_ASSERT(
    offsetof(BamlBuffer, len) >= sizeof(((BamlBuffer *)0)->ptr),
    "buffer length must follow pointer");
BAML_STATIC_ASSERT(offsetof(BamlBridgeInfoV1, struct_size) == 0, "size must be first");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, language) >= sizeof(((BamlBridgeInfoV1 *)0)->struct_size),
    "language must follow size");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, sdk_version) >=
        offsetof(BamlBridgeInfoV1, language) + sizeof(((BamlBridgeInfoV1 *)0)->language),
    "version pointer must follow language");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, sdk_version_len) >=
        offsetof(BamlBridgeInfoV1, sdk_version) + sizeof(((BamlBridgeInfoV1 *)0)->sdk_version),
    "version length must follow pointer");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, bridge_runtime_name) >=
        offsetof(BamlBridgeInfoV1, sdk_version_len) + sizeof(((BamlBridgeInfoV1 *)0)->sdk_version_len),
    "runtime name must follow toolchain version");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, bridge_runtime_name_len) >=
        offsetof(BamlBridgeInfoV1, bridge_runtime_name) + sizeof(((BamlBridgeInfoV1 *)0)->bridge_runtime_name),
    "runtime name length must follow pointer");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, bridge_runtime_version) >=
        offsetof(BamlBridgeInfoV1, bridge_runtime_name_len) + sizeof(((BamlBridgeInfoV1 *)0)->bridge_runtime_name_len),
    "runtime version must follow runtime name");
BAML_STATIC_ASSERT(
    offsetof(BamlBridgeInfoV1, bridge_runtime_version_len) >=
        offsetof(BamlBridgeInfoV1, bridge_runtime_version) + sizeof(((BamlBridgeInfoV1 *)0)->bridge_runtime_version),
    "runtime version length must follow pointer");

BAML_STATIC_ASSERT(offsetof(BamlApiV1, abi_version) == 0, "ABI version must be first");
BAML_ASSERT_AFTER(abi_version, struct_size);
BAML_ASSERT_AFTER(struct_size, version);
BAML_ASSERT_AFTER(version, initialize_runtime_from_bytecode);
BAML_ASSERT_AFTER(initialize_runtime_from_bytecode, free_buffer);
BAML_ASSERT_AFTER(free_buffer, register_callback);
BAML_ASSERT_AFTER(register_callback, call_function);
BAML_ASSERT_AFTER(call_function, new_function_call);
BAML_ASSERT_AFTER(new_function_call, cancel_function_call);
BAML_ASSERT_AFTER(cancel_function_call, register_host_dispatch_callback);
BAML_ASSERT_AFTER(register_host_dispatch_callback, register_host_release_callback);
BAML_ASSERT_AFTER(register_host_release_callback, complete_host_call);
BAML_ASSERT_AFTER(complete_host_call, handle_clone);
BAML_ASSERT_AFTER(handle_clone, handle_release);
BAML_ASSERT_AFTER(handle_release, media_from_url);
BAML_ASSERT_AFTER(media_from_url, media_from_file);
BAML_ASSERT_AFTER(media_from_file, media_from_base64);
BAML_ASSERT_AFTER(media_from_base64, media_url);
BAML_ASSERT_AFTER(media_url, media_file);
BAML_ASSERT_AFTER(media_file, media_base64);
BAML_ASSERT_AFTER(media_base64, media_mime_type);
BAML_ASSERT_AFTER(media_mime_type, register_bridge);
BAML_ASSERT_AFTER(register_bridge, register_unhandled_spawn_error_callback);
BAML_ASSERT_AFTER(register_unhandled_spawn_error_callback, shutdown_runtime);
BAML_ASSERT_AFTER(shutdown_runtime, initialize_runtime_from_bytecode_with_metadata);
BAML_STATIC_ASSERT(
    BAML_API_V1_MIN_SIZE == offsetof(BamlApiV1, register_unhandled_spawn_error_callback),
    "the appended lifecycle fields must follow the original V1 prefix");

BAML_ASSERT_FIELD_TYPE(version, BamlVersionFn)
BAML_ASSERT_FIELD_TYPE(initialize_runtime_from_bytecode, BamlInitializeRuntimeFromBytecodeFn)
BAML_ASSERT_FIELD_TYPE(free_buffer, BamlFreeBufferFn)
BAML_ASSERT_FIELD_TYPE(register_callback, BamlRegisterCallbackFn)
BAML_ASSERT_FIELD_TYPE(call_function, BamlCallFunctionFn)
BAML_ASSERT_FIELD_TYPE(new_function_call, BamlNewFunctionCallFn)
BAML_ASSERT_FIELD_TYPE(cancel_function_call, BamlCancelFunctionCallFn)
BAML_ASSERT_FIELD_TYPE(register_host_dispatch_callback, BamlRegisterHostDispatchCallbackFn)
BAML_ASSERT_FIELD_TYPE(register_host_release_callback, BamlRegisterHostReleaseCallbackFn)
BAML_ASSERT_FIELD_TYPE(complete_host_call, BamlCompleteHostCallFn)
BAML_ASSERT_FIELD_TYPE(handle_clone, BamlHandleCloneFn)
BAML_ASSERT_FIELD_TYPE(handle_release, BamlHandleReleaseFn)
BAML_ASSERT_FIELD_TYPE(media_from_url, BamlMediaConstructorFn)
BAML_ASSERT_FIELD_TYPE(media_from_file, BamlMediaConstructorFn)
BAML_ASSERT_FIELD_TYPE(media_from_base64, BamlMediaConstructorFn)
BAML_ASSERT_FIELD_TYPE(media_url, BamlMediaAccessorFn)
BAML_ASSERT_FIELD_TYPE(media_file, BamlMediaAccessorFn)
BAML_ASSERT_FIELD_TYPE(media_base64, BamlMediaAccessorFn)
BAML_ASSERT_FIELD_TYPE(media_mime_type, BamlMediaAccessorFn)
BAML_ASSERT_FIELD_TYPE(register_bridge, BamlRegisterBridgeFn)
BAML_ASSERT_FIELD_TYPE(register_unhandled_spawn_error_callback,
                       BamlRegisterUnhandledSpawnErrorCallbackFn)
BAML_ASSERT_FIELD_TYPE(shutdown_runtime, BamlShutdownRuntimeFn)
BAML_ASSERT_FIELD_TYPE(initialize_runtime_from_bytecode_with_metadata,
                       BamlInitializeRuntimeFromBytecodeWithMetadataFn)

#endif /* BAML_CFFI_TEST_ABI_ASSERTIONS_H */
