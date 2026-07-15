#ifndef BAML_DETAIL_CALL_H_
#define BAML_DETAIL_CALL_H_

// The call driver generated bindings sit on: encode CallFunctionArgs, cross
// the C ABI once, and hand back a Future correlated through the registry.

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

template <typename Ret>
Future<Ret> StartCall(const std::string& fqn, ArgsEncoder&& args) {
  CallRegistry::Started started = CallRegistry::Instance().Begin();
  const uint64_t engine_call_id = Api().new_function_call();
  const std::string encoded = args.Finish(engine_call_id);
  Api().call_function(fqn.c_str(),
                      reinterpret_cast<const uint8_t*>(encoded.data()),
                      encoded.size(), started.correlation_id);
  return Future<Ret>(std::move(started.envelope), engine_call_id);
}

template <typename Ret>
Ret CallSync(const std::string& fqn, ArgsEncoder&& args) {
  return StartCall<Ret>(fqn, std::move(args)).get();
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_CALL_H_
