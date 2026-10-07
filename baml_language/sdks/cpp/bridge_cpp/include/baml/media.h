#ifndef BAML_MEDIA_H_
#define BAML_MEDIA_H_

// Portable BAML media values. The protobuf payload (kind, optional MIME type,
// a URL or base64 content, and the name of the file the content was read from)
// is copied across the boundary; media is data, not an engine-owned
// capability handle. A media value never holds a path to read later: reading
// a file is the BAML function `from_file` of the media class, which the
// generated SDK exposes.

#include <baml/buffer.h>
#include <baml/codec.h>

#include <cstdint>
#include <optional>
#include <string>
#include <utility>

namespace baml {

enum class media_kind {
  generic = 0,
  image = 1,
  audio = 2,
  pdf = 3,
  video = 4,
};

namespace detail {

inline pb::MediaTypeEnum media_wire_kind(media_kind kind) {
  switch (kind) {
    case media_kind::image:
      return pb::IMAGE;
    case media_kind::audio:
      return pb::AUDIO;
    case media_kind::pdf:
      return pb::PDF;
    case media_kind::video:
      return pb::VIDEO;
    case media_kind::generic:
      return pb::MEDIA_TYPE_UNSPECIFIED;
  }
  return pb::MEDIA_TYPE_UNSPECIFIED;
}

inline media_kind media_host_kind(pb::MediaTypeEnum kind) {
  switch (kind) {
    case pb::IMAGE:
      return media_kind::image;
    case pb::AUDIO:
      return media_kind::audio;
    case pb::PDF:
      return media_kind::pdf;
    case pb::VIDEO:
      return media_kind::video;
    case pb::MEDIA_TYPE_UNSPECIFIED:
    case pb::OTHER:
      return media_kind::generic;
    default:
      return media_kind::generic;
  }
}

inline pb::BamlTyMediaKind media_ty_kind(media_kind kind) {
  switch (kind) {
    case media_kind::image:
      return pb::BAML_TY_MEDIA_KIND_IMAGE;
    case media_kind::audio:
      return pb::BAML_TY_MEDIA_KIND_AUDIO;
    case media_kind::video:
      return pb::BAML_TY_MEDIA_KIND_VIDEO;
    case media_kind::pdf:
      return pb::BAML_TY_MEDIA_KIND_PDF;
    case media_kind::generic:
      return pb::BAML_TY_MEDIA_KIND_GENERIC;
  }
  return pb::BAML_TY_MEDIA_KIND_UNSPECIFIED;
}

}  // namespace detail

template <media_kind Expected>
class basic_media {
 public:
  friend bool operator==(const basic_media& lhs, const basic_media& rhs) {
    return lhs.value_.media() == rhs.value_.media() &&
           lhs.value_.has_mime_type() == rhs.value_.has_mime_type() &&
           lhs.value_.mime_type() == rhs.value_.mime_type() &&
           lhs.value_.value_case() == rhs.value_.value_case() &&
           lhs.value_.url() == rhs.value_.url() &&
           lhs.value_.base64() == rhs.value_.base64() &&
           lhs.value_.file_content().name() ==
               rhs.value_.file_content().name() &&
           lhs.value_.file_content().base64() ==
               rhs.value_.file_content().base64();
  }

  friend bool operator!=(const basic_media& lhs, const basic_media& rhs) {
    return !(lhs == rhs);
  }

  // Constructors for concrete image/audio/video/pdf types.
  static basic_media from_url(
      std::string url, std::optional<std::string> mime_type = std::nullopt) {
    static_assert(Expected != media_kind::generic,
                  "generic media needs an explicit media kind");
    return from_url(Expected, std::move(url), std::move(mime_type));
  }

  static basic_media from_base64(
      std::string base64, std::optional<std::string> mime_type = std::nullopt) {
    static_assert(Expected != media_kind::generic,
                  "generic media needs an explicit media kind");
    return from_base64(Expected, std::move(base64), std::move(mime_type));
  }

  // Base64 content that was read from `file`: named by its base name, with the
  // MIME type it implies unless one is given. Reads nothing.
  static basic_media from_file_content(
      std::string file, std::string base64,
      std::optional<std::string> mime_type = std::nullopt) {
    static_assert(Expected != media_kind::generic,
                  "generic media needs an explicit media kind");
    return from_file_content(Expected, std::move(file), std::move(base64),
                             std::move(mime_type));
  }

  // Constructors for the generic `image | audio | video | pdf` host type.
  static basic_media from_url(
      media_kind kind, std::string url,
      std::optional<std::string> mime_type = std::nullopt) {
    return make(kind, std::move(mime_type), [&](detail::pb::BamlValueMedia& v) {
      v.set_url(std::move(url));
    });
  }

  static basic_media from_base64(
      media_kind kind, std::string base64,
      std::optional<std::string> mime_type = std::nullopt) {
    return make(kind, std::move(mime_type), [&](detail::pb::BamlValueMedia& v) {
      v.set_base64(std::move(base64));
    });
  }

  static basic_media from_file_content(
      media_kind kind, std::string file, std::string base64,
      std::optional<std::string> mime_type = std::nullopt) {
    check_kind(kind);
    // The runtime names the content and, with no MIME type given, infers
    // one: build the value there, read both back, and let its handle go.
    const BamlApiV1& table = detail::api();
    uint64_t key = 0;
    int32_t handle_type = 0;
    BamlCffiStatus status = table.media_from_file_content(
        static_cast<int32_t>(detail::media_wire_kind(kind)),
        c_string(file, "file"), c_string(base64, "base64"),
        optional_c_string(mime_type), &key, &handle_type);
    if (status != BAML_CFFI_STATUS_OK || key == 0) {
      throw error("BAML media from_file_content failed with status " +
                  std::to_string(static_cast<int>(status)));
    }
    struct release_handle {
      const BamlApiV1& table;
      uint64_t key;
      ~release_handle() { table.handle_release(key); }
    } guard{table, key};
    auto read = [&](BamlMediaAccessorFn accessor) {
      BamlBuffer buffer{nullptr, 0};
      if (accessor(key, handle_type, &buffer) != BAML_CFFI_STATUS_OK) {
        detail::owned_buffer release{buffer};
        throw error(
            "BAML media from_file_content could not read back the value it "
            "built");
      }
      return detail::owned_buffer(buffer);
    };
    detail::pb::BamlValueMedia value;
    value.set_media(detail::media_wire_kind(kind));
    detail::owned_buffer inferred_mime_type = read(table.media_mime_type);
    if (!inferred_mime_type.empty()) {
      value.set_mime_type(inferred_mime_type.to_string());
    }
    detail::owned_buffer name = read(table.media_name);
    if (name.empty()) {
      value.set_base64(std::move(base64));
    } else {
      detail::pb::BamlValueMediaFileContent* content =
          value.mutable_file_content();
      content->set_name(name.to_string());
      content->set_base64(std::move(base64));
    }
    return basic_media(std::move(value));
  }

  media_kind kind() const noexcept {
    return detail::media_host_kind(value_.media());
  }

  std::optional<std::string> mime_type() const {
    return value_.has_mime_type()
               ? std::optional<std::string>(value_.mime_type())
               : std::nullopt;
  }

  std::optional<std::string> url() const {
    return value_.value_case() == detail::pb::BamlValueMedia::kUrl
               ? std::optional<std::string>(value_.url())
               : std::nullopt;
  }

  std::optional<std::string> base64() const {
    switch (value_.value_case()) {
      case detail::pb::BamlValueMedia::kBase64:
        return value_.base64();
      case detail::pb::BamlValueMedia::kFileContent:
        return value_.file_content().base64();
      case detail::pb::BamlValueMedia::kUrl:
      case detail::pb::BamlValueMedia::VALUE_NOT_SET:
        return std::nullopt;
    }
    return std::nullopt;
  }

  // The base name of the file the content was read from, if any.
  std::optional<std::string> name() const {
    return value_.value_case() == detail::pb::BamlValueMedia::kFileContent
               ? std::optional<std::string>(value_.file_content().name())
               : std::nullopt;
  }

 private:
  explicit basic_media(detail::pb::BamlValueMedia value)
      : value_(std::move(value)) {}

  static void check_kind(media_kind kind) {
    if (kind == media_kind::generic ||
        (Expected != media_kind::generic && kind != Expected)) {
      throw error("invalid BAML media kind");
    }
  }

  static const char* c_string(const std::string& value, const char* field) {
    if (value.find('\0') != std::string::npos) {
      throw error(std::string("BAML media ") + field + " contains a NUL byte");
    }
    return value.c_str();
  }

  static const char* optional_c_string(
      const std::optional<std::string>& value) {
    return value ? c_string(*value, "MIME type") : nullptr;
  }

  template <typename SetValue>
  static basic_media make(media_kind kind, std::optional<std::string> mime_type,
                          SetValue&& set_value) {
    if (kind == media_kind::generic ||
        (Expected != media_kind::generic && kind != Expected)) {
      throw error("invalid BAML media kind");
    }
    detail::pb::BamlValueMedia value;
    value.set_media(detail::media_wire_kind(kind));
    if (mime_type) value.set_mime_type(std::move(*mime_type));
    set_value(value);
    return basic_media(std::move(value));
  }

  detail::pb::BamlValueMedia value_;

  friend struct codec<basic_media<Expected>>;
};

using media = basic_media<media_kind::generic>;
using image = basic_media<media_kind::image>;
using audio = basic_media<media_kind::audio>;
using pdf = basic_media<media_kind::pdf>;
using video = basic_media<media_kind::video>;

template <media_kind Expected>
struct codec<basic_media<Expected>> {
  static detail::pb::BamlTy baml_ty() {
    detail::pb::BamlTy ty;
    ty.mutable_media()->set_kind(detail::media_ty_kind(Expected));
    return ty;
  }

  static void encode(detail::pb::InboundValue& target,
                     const basic_media<Expected>& value) {
    target.mutable_media_value()->CopyFrom(value.value_);
    target.mutable_value_type()->CopyFrom(baml_ty());
  }

  static basic_media<Expected> decode(
      const detail::pb::BamlOutboundValue& raw) {
    const detail::pb::BamlOutboundValue& value = detail::unwrap(raw);
    if (value.value_case() != detail::pb::BamlOutboundValue::kMediaValue) {
      detail::kind_mismatch("media", value);
    }
    const media_kind actual =
        detail::media_host_kind(value.media_value().media());
    if (Expected != media_kind::generic && actual != Expected) {
      detail::kind_mismatch("specific media kind", value);
    }
    return basic_media<Expected>(value.media_value());
  }
};

}  // namespace baml

#endif  // BAML_MEDIA_H_
