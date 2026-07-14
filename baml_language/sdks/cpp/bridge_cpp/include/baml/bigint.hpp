#ifndef BAML_BIGINT_HPP
#define BAML_BIGINT_HPP

// baml::BigInt: BAML's arbitrary-precision integer as a canonical
// sign-magnitude hex string (the wire form: `[-]?[0-9a-f]+`, no 0x prefix).
// C++ has no built-in arbitrary-precision type, so the value stays in wire
// form; int64 conversions cover the practical range and throw outside it.

#include <cctype>
#include <cstdint>
#include <limits>
#include <string>
#include <utility>

#include <baml/errors.hpp>

namespace baml {

class BigInt {
public:
    BigInt() : hex_("0") {}

    explicit BigInt(int64_t v) {
        if (v < 0) {
            // Negate via uint64 so INT64_MIN does not overflow.
            hex_ = "-" + to_hex(~static_cast<uint64_t>(v) + 1);
        } else {
            hex_ = to_hex(static_cast<uint64_t>(v));
        }
    }

    // Parses the wire form. Throws BamlError on malformed input.
    static BigInt from_hex(const std::string& hex) {
        BigInt out;
        out.hex_ = normalize(hex);
        return out;
    }

    // Canonical wire form: lowercase, no leading zeros, "-" only when
    // non-zero negative.
    const std::string& hex() const { return hex_; }

    // Throws BamlError when the value does not fit in int64.
    int64_t to_int64() const {
        const bool negative = hex_[0] == '-';
        const std::string digits = negative ? hex_.substr(1) : hex_;
        if (digits.size() > 16) {
            throw BamlError("BigInt out of int64 range: " + hex_);
        }
        uint64_t magnitude = 0;
        for (char c : digits) {
            magnitude = (magnitude << 4) | static_cast<uint64_t>(hex_digit(c));
        }
        if (negative) {
            const uint64_t min_magnitude =
                static_cast<uint64_t>(std::numeric_limits<int64_t>::max()) + 1;
            if (magnitude > min_magnitude) {
                throw BamlError("BigInt out of int64 range: " + hex_);
            }
            return static_cast<int64_t>(~magnitude + 1);
        }
        if (magnitude > static_cast<uint64_t>(std::numeric_limits<int64_t>::max())) {
            throw BamlError("BigInt out of int64 range: " + hex_);
        }
        return static_cast<int64_t>(magnitude);
    }

    friend bool operator==(const BigInt& a, const BigInt& b) { return a.hex_ == b.hex_; }
    friend bool operator!=(const BigInt& a, const BigInt& b) { return !(a == b); }

private:
    static int hex_digit(char c) {
        if (c >= '0' && c <= '9') {
            return c - '0';
        }
        if (c >= 'a' && c <= 'f') {
            return c - 'a' + 10;
        }
        throw BamlError(std::string("invalid BigInt hex digit '") + c + "'");
    }

    static std::string to_hex(uint64_t v) {
        if (v == 0) {
            return "0";
        }
        static const char digits[] = "0123456789abcdef";
        std::string out;
        while (v != 0) {
            out.insert(out.begin(), digits[v & 0xf]);
            v >>= 4;
        }
        return out;
    }

    static std::string normalize(const std::string& raw) {
        size_t i = 0;
        bool negative = false;
        if (i < raw.size() && (raw[i] == '-' || raw[i] == '+')) {
            negative = raw[i] == '-';
            ++i;
        }
        if (i >= raw.size()) {
            throw BamlError("empty BigInt hex string");
        }
        std::string digits;
        digits.reserve(raw.size() - i);
        for (; i < raw.size(); ++i) {
            const char c = raw[i];
            const char lower =
                static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            (void)hex_digit(lower);  // validates
            digits.push_back(lower);
        }
        const size_t first = digits.find_first_not_of('0');
        if (first == std::string::npos) {
            return "0";
        }
        digits.erase(0, first);
        return negative ? "-" + digits : digits;
    }

    std::string hex_;
};

}  // namespace baml

#endif  // BAML_BIGINT_HPP
