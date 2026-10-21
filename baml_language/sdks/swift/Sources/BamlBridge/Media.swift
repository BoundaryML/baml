import CBamlBridge
import Foundation

/// Media constructors over the C ABI. The generated media structs
/// (`Baml.baml.media.Image`, …) hold a single `_data: BamlHandle?`;
/// their accessors (`mime_type()`, `base64()`, …) and `from_file`, which
/// reads a file, are generated engine calls. Building a value from a URL
/// or from content needs no engine, and is what these do.
public enum BamlMedia {
    /// Raw values are `BamlCffiMediaKind` — the canonical V1 ABI values
    /// (shared with the protobuf `MediaTypeEnum`; zero is reserved).
    public enum Kind: Int32, Sendable {
        case image = 1
        case audio = 2
        case pdf = 3
        case video = 4
        case generic = 5
    }

    /// What a media value is built from.
    private enum Source {
        case url(String)
        /// Base64 content read from the file at `file`. Reads nothing.
        case fileContent(file: String, base64: String)
        case base64(String)
    }

    /// What a media accessor reads.
    private enum Field {
        case url
        case name
        case base64
        case mimeType
    }

    private static func construct(
        _ kind: Kind,
        _ source: Source,
        mimeType: String?
    ) throws -> BamlHandle {
        var key: UInt64 = 0
        var handleType: Int32 = 0
        let status: UInt32 = withOptionalCString(mimeType) { mime in
            switch source {
            case .url(let url):
                return url.withCString { BamlApi.mediaFromUrl(kind.rawValue, $0, mime, &key, &handleType) }
            case .fileContent(let file, let base64):
                return file.withCString { file in
                    base64.withCString {
                        BamlApi.mediaFromFileContent(kind.rawValue, file, $0, mime, &key, &handleType)
                    }
                }
            case .base64(let base64):
                return base64.withCString { BamlApi.mediaFromBase64(kind.rawValue, $0, mime, &key, &handleType) }
            }
        }
        guard status == BAML_CFFI_STATUS_OK.rawValue else {
            throw BamlDecodeError.unsupported("media construction failed with status \(status)")
        }
        guard let wireType = BamlBridge_Cffi_V1_BamlHandleType(rawValue: Int(handleType)) else {
            throw BamlDecodeError.unsupported("unknown media handle type \(handleType)")
        }
        var portable = BamlBridge_Cffi_V1_BamlValueMedia()
        portable.media = BamlBridge_Cffi_V1_MediaTypeEnum(rawValue: Int(kind.rawValue))!
        switch source {
        case .url(let url):
            if let mimeType { portable.mimeType = mimeType }
            portable.url = url
        case .base64(let base64):
            if let mimeType { portable.mimeType = mimeType }
            portable.base64 = base64
        case .fileContent(_, let base64):
            // The runtime named the content and, with no MIME type given,
            // inferred one: read both back. Nothing owns the handle yet, so a
            // failed read has to let it go.
            do {
                if let mimeType = try read(.mimeType, key: key, handleType: wireType) {
                    portable.mimeType = mimeType
                }
                if let name = try read(.name, key: key, handleType: wireType) {
                    portable.fileContent = .with {
                        $0.name = name
                        $0.base64 = base64
                    }
                } else {
                    portable.base64 = base64
                }
            } catch {
                _ = BamlApi.handleRelease(key)
                throw error
            }
        }
        return BamlHandle(key: key, handleType: wireType, portableMedia: portable)
    }

    private static func withOptionalCString<T>(
        _ value: String?,
        _ body: (UnsafePointer<CChar>?) -> T
    ) -> T {
        guard let value else { return body(nil) }
        return value.withCString { body($0) }
    }

    public static func fromUrl(_ kind: Kind, _ url: String, mimeType: String?) throws -> BamlHandle {
        try construct(kind, .url(url), mimeType: mimeType)
    }

    /// Base64 content that was read from `file`: named by its base name,
    /// with the MIME type it implies unless one is given. Reads nothing.
    public static func fromFileContent(
        _ kind: Kind,
        file: String,
        base64: String,
        mimeType: String?
    ) throws -> BamlHandle {
        try construct(kind, .fileContent(file: file, base64: base64), mimeType: mimeType)
    }

    /// Mint a media handle from base64 payload — wrap the result in
    /// the generated struct: `Image(_data: try BamlMedia.fromBase64(...))`.
    public static func fromBase64(
        _ kind: Kind,
        _ base64: String,
        mimeType: String?
    ) throws -> BamlHandle {
        try construct(kind, .base64(base64), mimeType: mimeType)
    }

    static func isPortableHandle(_ type: BamlBridge_Cffi_V1_BamlHandleType) -> Bool {
        type == .adtMediaImage || type == .adtMediaAudio
            || type == .adtMediaVideo || type == .adtMediaPdf
    }

    private static func kind(for type: BamlBridge_Cffi_V1_BamlHandleType) throws -> Kind {
        switch type {
        case .adtMediaImage: return .image
        case .adtMediaAudio: return .audio
        case .adtMediaVideo: return .video
        case .adtMediaPdf: return .pdf
        default: throw BamlDecodeError.typeMismatch(expected: "media handle", got: "handle type \(type)")
        }
    }

    private static func read(
        _ field: Field,
        key: UInt64,
        handleType: BamlBridge_Cffi_V1_BamlHandleType
    ) throws -> String? {
        var buffer = BamlBuffer(ptr: nil, len: 0)
        let status: UInt32
        switch field {
        case .url: status = BamlApi.mediaUrl(key, Int32(handleType.rawValue), &buffer)
        case .name: status = BamlApi.mediaName(key, Int32(handleType.rawValue), &buffer)
        case .base64: status = BamlApi.mediaBase64(key, Int32(handleType.rawValue), &buffer)
        case .mimeType: status = BamlApi.mediaMimeType(key, Int32(handleType.rawValue), &buffer)
        }
        guard status == BAML_CFFI_STATUS_OK.rawValue else {
            throw BamlDecodeError.unsupported("media access failed with status \(status)")
        }
        let absent = buffer.ptr == nil
        let data = BamlApi.takeBuffer(buffer)
        guard !absent else { return nil }
        guard let value = String(data: data, encoding: .utf8) else {
            throw BamlDecodeError.typeMismatch(expected: "UTF-8 media data", got: "invalid bytes")
        }
        return value
    }

    static func snapshotPortable(
        key: UInt64,
        handleType: BamlBridge_Cffi_V1_BamlHandleType
    ) throws -> BamlBridge_Cffi_V1_BamlValueMedia {
        let kind = try kind(for: handleType)
        var media = BamlBridge_Cffi_V1_BamlValueMedia()
        media.media = BamlBridge_Cffi_V1_MediaTypeEnum(rawValue: Int(kind.rawValue))!
        if let mimeType = try read(.mimeType, key: key, handleType: handleType) {
            media.mimeType = mimeType
        }
        if let url = try read(.url, key: key, handleType: handleType) {
            media.url = url
        } else if let base64 = try read(.base64, key: key, handleType: handleType) {
            // Content read from a file keeps the file's name.
            if let name = try read(.name, key: key, handleType: handleType) {
                media.fileContent = .with {
                    $0.name = name
                    $0.base64 = base64
                }
            } else {
                media.base64 = base64
            }
        } else {
            throw BamlDecodeError.typeMismatch(expected: "media payload", got: "empty media")
        }
        return media
    }

    static func encodePortable(_ media: BamlBridge_Cffi_V1_BamlValueMedia) -> BamlInboundValue {
        var inbound = BamlBridge_Cffi_V1_InboundValue()
        inbound.mediaValue = media
        return BamlInboundValue(inbound)
    }

    static func decodePortable(_ media: BamlBridge_Cffi_V1_BamlValueMedia) throws -> BamlHandle {
        guard let kind = Kind(rawValue: Int32(media.media.rawValue)) else {
            throw BamlDecodeError.unsupported("unknown media kind \(media.media.rawValue)")
        }
        let mimeType = media.hasMimeType ? media.mimeType : nil
        switch media.value {
        case .url(let value): return try fromUrl(kind, value, mimeType: mimeType)
        case .base64(let value): return try fromBase64(kind, value, mimeType: mimeType)
        case .fileContent(let content):
            return try fromFileContent(kind, file: content.name, base64: content.base64, mimeType: mimeType)
        case nil: throw BamlDecodeError.typeMismatch(expected: "media payload", got: "empty media")
        }
    }
}
