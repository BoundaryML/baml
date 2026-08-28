#ifndef BAML_DETAIL_CALL_H_
#define BAML_DETAIL_CALL_H_

// The call drivers generated bindings sit on: encode CallFunctionArgs,
// cross the C ABI once, and correlate the result envelope through the
// registry. start_call returns the in-flight call as a baml::future; the
// synchronous form is exactly start_call + an immediate blocking get(), so
// both spellings share one code path.

#include <baml/codec.h>
#include <baml/detail/proto.h>
#include <baml/detail/registry.h>
#include <baml/future.h>
#include <baml_cffi.h>

#include <cstdint>
#include <string>
#include <utility>

namespace baml {
namespace detail {

// ThrownU is the function's declared `throws` set as a baml::variant (void
// when the function declares none): the error arm then surfaces as
// thrown<ThrownU> instead of an untyped error.
template <typename Ret, typename ThrownU = void>
future<Ret, ThrownU> start_call(
    const std::string& fqn, args_encoder&& args,
    function_operation operation = function_operation::direct) {
  call_registry::started started = call_registry::instance().begin();
  const uint64_t engine_call_id = api().new_function_call();
  const std::string encoded = args.finish(engine_call_id, fqn, operation);
  api().call_function(reinterpret_cast<const uint8_t*>(encoded.data()),
                      encoded.size(), started.correlation_id);
  return future<Ret, ThrownU>(std::move(started.state), engine_call_id);
}

template <typename Ret, typename ThrownU = void>
future<Ret, ThrownU> start_handle_call(uint64_t handle_key,
                                       args_encoder&& args,
                                       function_operation operation =
                                           function_operation::direct) {
  call_registry::started started = call_registry::instance().begin();
  const uint64_t engine_call_id = api().new_function_call();
  const std::string encoded = args.finish(engine_call_id, handle_key, operation);
  api().call_function(reinterpret_cast<const uint8_t*>(encoded.data()),
                      encoded.size(), started.correlation_id);
  return future<Ret, ThrownU>(std::move(started.state), engine_call_id);
}

template <typename Ret, typename ThrownU = void>
Ret call_sync(const std::string& fqn, args_encoder&& args,
              function_operation operation = function_operation::direct) {
  return start_call<Ret, ThrownU>(fqn, std::move(args), operation).get();
}

template <typename Ret, typename ThrownU>
Ret call_handle_sync(uint64_t handle_key, args_encoder&& args,
                     function_operation operation) {
  return start_handle_call<Ret, ThrownU>(handle_key, std::move(args), operation)
      .get();
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_CALL_H_
