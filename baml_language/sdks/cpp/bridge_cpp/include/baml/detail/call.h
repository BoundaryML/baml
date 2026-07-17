#ifndef BAML_DETAIL_CALL_H_
#define BAML_DETAIL_CALL_H_

// The call driver generated bindings sit on: encode CallFunctionArgs, cross
// the C ABI once, and block on the result envelope correlated through the
// registry. (The engine call model is asynchronous; this slice exposes only
// the synchronous wrapper.)

#include <baml/codec.h>
#include <baml/detail/proto.h>
#include <baml/detail/registry.h>
#include <baml_cffi.h>

#include <cstdint>
#include <string>

namespace baml {
namespace detail {

// ThrownU is the function's declared `throws` set as a baml::Union (void
// when the function declares none): the error arm then surfaces as
// BamlThrown<ThrownU> instead of an untyped BamlError.
template <typename Ret, typename ThrownU = void>
Ret CallSync(const std::string& fqn, ArgsEncoder&& args) {
  CallRegistry::Started started = CallRegistry::Instance().Begin();
  const uint64_t engine_call_id = Api().new_function_call();
  const std::string encoded = args.Finish(engine_call_id);
  Api().call_function(fqn.c_str(),
                      reinterpret_cast<const uint8_t*>(encoded.data()),
                      encoded.size(), started.correlation_id);
  return DecodeResult<Ret, ThrownU>(started.envelope.get());
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_CALL_H_
