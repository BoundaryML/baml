#include "abi_assertions.h"

int main() {
  static_assert(std::is_same<decltype(&baml_get_api_v1), BamlGetApiV1Fn>::value);
  return 0;
}
