#pragma once

#include <filesystem>
#include <string>
#include <vector>

namespace gloo::Generate {

#define PYTHONIC() void toPython(const std::vector<std::string>& deps) const
#define IMPL_PYTHONIC(cls) \
  void cls::toPython(const std::vector<std::string>& deps) const

class PythonImpl {
 public:
  virtual ~PythonImpl() = default;
  virtual PYTHONIC() = 0;
};

}  // namespace gloo::Generate