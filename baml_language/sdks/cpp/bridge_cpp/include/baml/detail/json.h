#ifndef BAML_DETAIL_JSON_H_
#define BAML_DETAIL_JSON_H_

#include <cstdio>
#include <map>
#include <string>

namespace baml {
namespace detail {

// Minimal JSON string escaping for the src_files map handed to
// create_baml_runtime (the one JSON surface in the C ABI). UTF-8 bytes pass
// through untouched; only quotes, backslashes, and control chars are escaped.
inline void JsonEscapeInto(std::string& out, const std::string& s) {
  for (unsigned char c : s) {
    switch (c) {
      case '"':
        out += "\\\"";
        break;
      case '\\':
        out += "\\\\";
        break;
      case '\b':
        out += "\\b";
        break;
      case '\f':
        out += "\\f";
        break;
      case '\n':
        out += "\\n";
        break;
      case '\r':
        out += "\\r";
        break;
      case '\t':
        out += "\\t";
        break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out += static_cast<char>(c);
        }
    }
  }
}

inline std::string JsonEncodeStringMap(
    const std::map<std::string, std::string>& m) {
  std::string out = "{";
  bool first = true;
  for (const auto& entry : m) {
    if (!first) {
      out += ",";
    }
    first = false;
    out += "\"";
    JsonEscapeInto(out, entry.first);
    out += "\":\"";
    JsonEscapeInto(out, entry.second);
    out += "\"";
  }
  out += "}";
  return out;
}

}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_JSON_H_
