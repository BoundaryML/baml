import CBamlBridge
import Foundation

/// Media constructors over the C ABI. The generated media structs
/// (`Baml.baml.media.Image`, …) hold a single `_data: BamlHandle?`;
/// their accessors (`mime_type()`, `base64()`, …) are generated engine
/// calls — only *construction* is a native op (`Image.from_base64` etc.
/// are VM-native methods that never enter the codegen pool; Python
/// exposes them through its PyO3 wrapper the same way).
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

    private enum Source {
        case url
        case file
        case base64
        case mimeType
    }

    private static func construct(
        _ kind: Kind,
        value: String,
        mimeType: String?,
        source: Source
    ) throws -> BamlHandle {
        var key: UInt64 = 0
        var handleType: Int32 = 0
        let invoke: (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> UInt32 = { raw, mime in
            switch source {
            case .url: return BamlApi.mediaFromUrl(kind.rawValue, raw, mime, &key, &handleType)
            case .file: return BamlApi.mediaFromFile(kind.rawValue, raw, mime, &key, &handleType)
            case .base64: return BamlApi.mediaFromBase64(kind.rawValue, raw, mime, &key, &handleType)
            case .mimeType: preconditionFailure("MIME type is not a media constructor")
            }
        }
        let status = value.withCString { raw in
            if let mimeType {
                return mimeType.withCString { invoke(raw, $0) }
            }
            return invoke(raw, nil)
        }
        guard status == BAML_CFFI_STATUS_OK.rawValue else {
            throw BamlDecodeError.unsupported("media construction failed with status \(status)")
        }
        guard let wireType = BamlBridge_Cffi_V1_BamlHandleType(rawValue: Int(handleType)) else {
            throw BamlDecodeError.unsupported("unknown media handle type \(handleType)")
        }
        return BamlHandle(key: key, handleType: wireType)
    }

    public static func fromUrl(_ kind: Kind, _ url: String, mimeType: String?) throws -> BamlHandle {
        try construct(kind, value: url, mimeType: mimeType, source: .url)
    }

    public static func fromFile(_ kind: Kind, _ file: String, mimeType: String?) throws -> BamlHandle {
        try construct(kind, value: file, mimeType: mimeType, source: .file)
    }

    /// Mint a media handle from base64 payload — wrap the result in
    /// the generated struct: `Image(_data: try BamlMedia.fromBase64(...))`.
    public static func fromBase64(
        _ kind: Kind,
        _ base64: String,
        mimeType: String?
    ) throws -> BamlHandle {
        try construct(kind, value: base64, mimeType: mimeType, source: .base64)
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

    private static func read(_ source: Source, handle: BamlHandle) throws -> String? {
        var buffer = BamlBuffer(ptr: nil, len: 0)
        let status: UInt32
        switch source {
        case .url: status = BamlApi.mediaUrl(handle.key, Int32(handle.handleType.rawValue), &buffer)
        case .file: status = BamlApi.mediaFile(handle.key, Int32(handle.handleType.rawValue), &buffer)
        case .base64: status = BamlApi.mediaBase64(handle.key, Int32(handle.handleType.rawValue), &buffer)
        case .mimeType: status = BamlApi.mediaMimeType(handle.key, Int32(handle.handleType.rawValue), &buffer)
        }
        guard status == BAML_CFFI_STATUS_OK.rawValue else {
            throw BamlDecodeError.unsupported("media access failed with status \(status)")
        }
        let absent = buffer.ptr == nil
        let data = BamlApi.takeBuffer(buffer)
        return absent ? nil : String(decoding: data, as: UTF8.self)
    }

    static func encodePortable(_ handle: BamlHandle) -> BamlInboundValue {
        do {
            let kind = try kind(for: handle.handleType)
            var media = BamlBridge_Cffi_V1_BamlValueMedia()
            media.media = BamlBridge_Cffi_V1_MediaTypeEnum(rawValue: Int(kind.rawValue))!
            if let mimeType = try read(.mimeType, handle: handle) { media.mimeType = mimeType }
            if let url = try read(.url, handle: handle) {
                media.url = url
            } else if let file = try read(.file, handle: handle) {
                media.file = file
            } else if let base64 = try read(.base64, handle: handle) {
                media.base64 = base64
            } else {
                preconditionFailure("BAML media value has no portable payload")
            }
            var inbound = BamlBridge_Cffi_V1_InboundValue()
            inbound.mediaValue = media
            return BamlInboundValue(inbound)
        } catch {
            preconditionFailure("failed to serialize BAML media: \(error)")
        }
    }

    static func decodePortable(_ media: BamlBridge_Cffi_V1_BamlValueMedia) throws -> BamlHandle {
        guard let kind = Kind(rawValue: Int32(media.media.rawValue)) else {
            throw BamlDecodeError.unsupported("unknown media kind \(media.media.rawValue)")
        }
        let mimeType = media.hasMimeType ? media.mimeType : nil
        switch media.value {
        case .url(let value): return try fromUrl(kind, value, mimeType: mimeType)
        case .file(let value): return try fromFile(kind, value, mimeType: mimeType)
        case .base64(let value): return try fromBase64(kind, value, mimeType: mimeType)
        case nil: throw BamlDecodeError.typeMismatch(expected: "media payload", got: "empty media")
        }
    }
}
