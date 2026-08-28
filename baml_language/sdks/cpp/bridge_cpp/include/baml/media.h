#ifndef BAML_MEDIA_H_
#define BAML_MEDIA_H_

// Portable BAML media values. The protobuf payload (kind, optional MIME type,
// and URL/base64/file source) is copied across the boundary; media is data,
// not an engine-owned capability handle.

#include <baml/codec.h>

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
           lhs.value_.file() == rhs.value_.file();
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
      std::string base64,
      std::optional<std::string> mime_type = std::nullopt) {
    static_assert(Expected != media_kind::generic,
                  "generic media needs an explicit media kind");
    return from_base64(Expected, std::move(base64), std::move(mime_type));
  }

  static basic_media from_file(
      std::string file, std::optional<std::string> mime_type = std::nullopt) {
    static_assert(Expected != media_kind::generic,
                  "generic media needs an explicit media kind");
    return from_file(Expected, std::move(file), std::move(mime_type));
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

  static basic_media from_file(
      media_kind kind, std::string file,
      std::optional<std::string> mime_type = std::nullopt) {
    return make(kind, std::move(mime_type), [&](detail::pb::BamlValueMedia& v) {
      v.set_file(std::move(file));
    });
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
    return value_.value_case() == detail::pb::BamlValueMedia::kBase64
               ? std::optional<std::string>(value_.base64())
               : std::nullopt;
  }

  std::optional<std::string> file() const {
    return value_.value_case() == detail::pb::BamlValueMedia::kFile
               ? std::optional<std::string>(value_.file())
               : std::nullopt;
  }

 private:
  explicit basic_media(detail::pb::BamlValueMedia value)
      : value_(std::move(value)) {}

  template <typename SetValue>
  static basic_media make(media_kind kind,
                          std::optional<std::string> mime_type,
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
    const media_kind actual = detail::media_host_kind(value.media_value().media());
    if (Expected != media_kind::generic && actual != Expected) {
      detail::kind_mismatch("specific media kind", value);
    }
    return basic_media<Expected>(value.media_value());
  }
};

}  // namespace baml

#endif  // BAML_MEDIA_H_
