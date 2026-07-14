#ifndef BAML_DETAIL_WIRE_HPP
#define BAML_DETAIL_WIRE_HPP

// Minimal protobuf wire-format reader/writer, hand-rolled so the bridge has
// no protobuf runtime dependency (see the codegen spec's packaging decision).
// Covers exactly what the bridge_ctypes schemas use: varint, 64-bit fixed,
// and length-delimited fields.

#include <cstdint>
#include <cstring>
#include <string>

#include <baml/errors.hpp>

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
    void varint(uint64_t v) {
        while (v >= 0x80) {
            buf_.push_back(static_cast<char>((v & 0x7f) | 0x80));
            v >>= 7;
        }
        buf_.push_back(static_cast<char>(v));
    }

    void tag(uint32_t field, WireType wt) {
        varint((static_cast<uint64_t>(field) << 3) | static_cast<uint64_t>(wt));
    }

    // proto3 scalar fields elide default values; the callers below write
    // unconditionally where presence is carried by a oneof.
    void int64_field(uint32_t field, int64_t v) {
        tag(field, WireType::Varint);
        varint(static_cast<uint64_t>(v));
    }

    void uint64_field(uint32_t field, uint64_t v) {
        tag(field, WireType::Varint);
        varint(v);
    }

    void bool_field(uint32_t field, bool v) {
        tag(field, WireType::Varint);
        varint(v ? 1 : 0);
    }

    void double_field(uint32_t field, double v) {
        tag(field, WireType::Fixed64);
        char raw[8];
        std::memcpy(raw, &v, 8);
        buf_.append(raw, 8);
    }

    void bytes_field(uint32_t field, const void* data, size_t len) {
        tag(field, WireType::Len);
        varint(len);
        buf_.append(static_cast<const char*>(data), len);
    }

    void string_field(uint32_t field, const std::string& s) {
        bytes_field(field, s.data(), s.size());
    }

    void message_field(uint32_t field, const Writer& sub) {
        bytes_field(field, sub.buf_.data(), sub.buf_.size());
    }

    const std::string& bytes() const { return buf_; }
    bool empty() const { return buf_.empty(); }

private:
    std::string buf_;
};

class Reader {
public:
    Reader(const uint8_t* data, size_t len) : p_(data), end_(data + len) {}

    bool done() const { return p_ >= end_; }

    // Reads the next field header. Returns false at end of message.
    bool next(uint32_t& field, WireType& wt) {
        if (done()) {
            return false;
        }
        const uint64_t key = varint();
        field = static_cast<uint32_t>(key >> 3);
        wt = static_cast<WireType>(key & 0x7);
        return true;
    }

    uint64_t varint() {
        uint64_t out = 0;
        int shift = 0;
        while (true) {
            if (p_ >= end_ || shift >= 64) {
                fail("truncated or overlong varint");
            }
            const uint8_t byte = *p_++;
            out |= static_cast<uint64_t>(byte & 0x7f) << shift;
            if ((byte & 0x80) == 0) {
                return out;
            }
            shift += 7;
        }
    }

    int64_t int64() { return static_cast<int64_t>(varint()); }
    bool boolean() { return varint() != 0; }

    double fixed64_double() {
        if (end_ - p_ < 8) {
            fail("truncated fixed64");
        }
        double v;
        std::memcpy(&v, p_, 8);
        p_ += 8;
        return v;
    }

    // Returns the payload of a length-delimited field.
    Reader len_payload() {
        const uint64_t len = varint();
        if (static_cast<uint64_t>(end_ - p_) < len) {
            fail("truncated length-delimited field");
        }
        Reader sub(p_, static_cast<size_t>(len));
        p_ += len;
        return sub;
    }

    std::string len_string() {
        Reader sub = len_payload();
        return std::string(reinterpret_cast<const char*>(sub.p_),
                           static_cast<size_t>(sub.end_ - sub.p_));
    }

    const uint8_t* data() const { return p_; }
    size_t size() const { return static_cast<size_t>(end_ - p_); }

    void skip(WireType wt) {
        switch (wt) {
            case WireType::Varint: varint(); break;
            case WireType::Fixed64:
                if (end_ - p_ < 8) {
                    fail("truncated fixed64 skip");
                }
                p_ += 8;
                break;
            case WireType::Len: len_payload(); break;
            case WireType::Fixed32:
                if (end_ - p_ < 4) {
                    fail("truncated fixed32 skip");
                }
                p_ += 4;
                break;
        }
    }

    [[noreturn]] static void fail(const char* what) {
        throw BamlError(std::string("BAML wire decode error: ") + what);
    }

private:
    const uint8_t* p_;
    const uint8_t* end_;
};

}  // namespace wire
}  // namespace detail
}  // namespace baml

#endif  // BAML_DETAIL_WIRE_HPP
