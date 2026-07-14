#ifndef BAML_DETAIL_CALL_HPP
#define BAML_DETAIL_CALL_HPP

// The call driver generated bindings sit on: encode CallFunctionArgs, cross
// the C ABI once, and hand back a Future correlated through the registry.

#include <cstdint>
#include <string>
#include <utility>

#include <baml_cffi.h>

#include <baml/codec.hpp>
#include <baml/detail/proto.hpp>
#include <baml/detail/registry.hpp>
#include <baml/future.hpp>

namespace baml {
namespace detail {

template <typename Ret>
Future<Ret> start_call(const std::string& fqn, ArgsEncoder&& args) {
    CallRegistry::Started started = CallRegistry::instance().begin();
    const uint64_t engine_call_id = new_function_call();
    const std::string encoded = args.finish(engine_call_id);
    call_function(fqn.c_str(), reinterpret_cast<const uint8_t*>(encoded.data()),
                  encoded.size(), started.correlation_id);
    return Future<Ret>(std::move(started.envelope), engine_call_id);
}

template <typename Ret>
Ret call_sync(const std::string& fqn, ArgsEncoder&& args) {
    return start_call<Ret>(fqn, std::move(args)).get();
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_CALL_HPP
