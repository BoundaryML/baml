#ifndef BAML_DETAIL_CALL_H_
#define BAML_DETAIL_CALL_H_

// The call drivers generated bindings sit on: encode CallFunctionArgs,
// cross the C ABI once, and correlate the result envelope through the
// registry. StartCall returns the in-flight call as a baml::Future; the
// synchronous form is exactly StartCall + an immediate blocking get(), so
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

// ThrownU is the function's declared `throws` set as a baml::Union (void
// when the function declares none): the error arm then surfaces as
// BamlThrown<ThrownU> instead of an untyped BamlError.
template <typename Ret, typename ThrownU = void>
Future<Ret, ThrownU> StartCall(const std::string& fqn, ArgsEncoder&& args) {
  CallRegistry::Started started = CallRegistry::Instance().Begin();
  const uint64_t engine_call_id = Api().new_function_call();
  const std::string encoded = args.Finish(engine_call_id);
  Api().call_function(fqn.c_str(),
                      reinterpret_cast<const uint8_t*>(encoded.data()),
                      encoded.size(), started.correlation_id);
  return Future<Ret, ThrownU>(std::move(started.state), engine_call_id);
}

template <typename Ret, typename ThrownU = void>
Ret CallSync(const std::string& fqn, ArgsEncoder&& args) {
  return StartCall<Ret, ThrownU>(fqn, std::move(args)).get();
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_CALL_H_
