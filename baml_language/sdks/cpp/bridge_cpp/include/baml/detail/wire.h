#ifndef BAML_DETAIL_WIRE_H_
#define BAML_DETAIL_WIRE_H_

// Minimal protobuf wire-format reader/writer, hand-rolled so the bridge has
// no protobuf runtime dependency (see the codegen spec's packaging decision).
// Covers exactly what the bridge_ctypes schemas use: varint, 64-bit fixed,
// and length-delimited fields.

#include <baml/errors.h>

#include <cstdint>
#include <cstring>
#include <string>

namespace baml {
namespace detail {
namespace wire {

enum class WireType : uint32_t {
  Varint = 0,
  Fixed64 = 1,
  Len = 2,
  Fixed32 = 5,
};

class Writer {
 public:
  void Varint(uint64_t v) {
    while (v >= 0x80) {
      buf_.push_back(static_cast<char>((v & 0x7f) | 0x80));
      v >>= 7;
    }
    buf_.push_back(static_cast<char>(v));
  }

  void Tag(uint32_t field, WireType wt) {
    Varint((static_cast<uint64_t>(field) << 3) | static_cast<uint64_t>(wt));
  }

  // proto3 scalar fields elide default values; the callers below write
  // unconditionally where presence is carried by a oneof.
  void Int64Field(uint32_t field, int64_t v) {
    Tag(field, WireType::Varint);
    Varint(static_cast<uint64_t>(v));
  }

  void Uint64Field(uint32_t field, uint64_t v) {
    Tag(field, WireType::Varint);
    Varint(v);
  }

  void BoolField(uint32_t field, bool v) {
    Tag(field, WireType::Varint);
    Varint(v ? 1 : 0);
  }

  void DoubleField(uint32_t field, double v) {
    Tag(field, WireType::Fixed64);
    char raw[8];
    std::memcpy(raw, &v, 8);
    buf_.append(raw, 8);
  }

  void BytesField(uint32_t field, const void* data, size_t len) {
    Tag(field, WireType::Len);
    Varint(len);
    buf_.append(static_cast<const char*>(data), len);
  }

  void StringField(uint32_t field, const std::string& s) {
    BytesField(field, s.data(), s.size());
  }

  void MessageField(uint32_t field, const Writer& sub) {
    BytesField(field, sub.buf_.data(), sub.buf_.size());
  }

  const std::string& bytes() const { return buf_; }

 private:
  std::string buf_;
};

class Reader {
 public:
  Reader(const uint8_t* data, size_t len) : p_(data), end_(data + len) {}

  bool done() const { return p_ >= end_; }

  // Reads the next field header. Returns false at end of message.
  bool Next(uint32_t& field, WireType& wt) {
    if (done()) {
      return false;
    }
    const uint64_t key = Varint();
    field = static_cast<uint32_t>(key >> 3);
    wt = static_cast<WireType>(key & 0x7);
    return true;
  }

  uint64_t Varint() {
    uint64_t out = 0;
    int shift = 0;
    while (true) {
      if (p_ >= end_ || shift >= 64) {
        Fail("truncated or overlong varint");
      }
      const uint8_t byte = *p_++;
      out |= static_cast<uint64_t>(byte & 0x7f) << shift;
      if ((byte & 0x80) == 0) {
        return out;
      }
      shift += 7;
    }
  }

  int64_t Int64() { return static_cast<int64_t>(Varint()); }
  bool Boolean() { return Varint() != 0; }

  double Fixed64Double() {
    if (end_ - p_ < 8) {
      Fail("truncated fixed64");
    }
    double v;
    std::memcpy(&v, p_, 8);
    p_ += 8;
    return v;
  }

  // Returns the payload of a length-delimited field.
  Reader LenPayload() {
    const uint64_t len = Varint();
    if (static_cast<uint64_t>(end_ - p_) < len) {
      Fail("truncated length-delimited field");
    }
    Reader sub(p_, static_cast<size_t>(len));
    p_ += len;
    return sub;
  }

  std::string LenString() {
    Reader sub = LenPayload();
    return std::string(reinterpret_cast<const char*>(sub.p_),
                       static_cast<size_t>(sub.end_ - sub.p_));
  }

  const uint8_t* data() const { return p_; }
  size_t size() const { return static_cast<size_t>(end_ - p_); }

  void Skip(WireType wt) {
    switch (wt) {
      case WireType::Varint:
        Varint();
        break;
      case WireType::Fixed64:
        if (end_ - p_ < 8) {
          Fail("truncated fixed64 skip");
        }
        p_ += 8;
        break;
      case WireType::Len:
        LenPayload();
        break;
      case WireType::Fixed32:
        if (end_ - p_ < 4) {
          Fail("truncated fixed32 skip");
        }
        p_ += 4;
        break;
    }
  }

  [[noreturn]] static void Fail(const char* what) {
    throw BamlError(std::string("BAML wire decode error: ") + what);
  }

 private:
  const uint8_t* p_;
  const uint8_t* end_;
};

}  // namespace wire
}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_WIRE_H_
