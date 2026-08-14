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

    /// Mint a media handle from base64 payload — wrap the result in
    /// the generated struct: `Image(_data: try BamlMedia.fromBase64(...))`.
    public static func fromBase64(
        _ kind: Kind,
        _ base64: String,
        mimeType: String?
    ) throws -> BamlHandle {
        var key: UInt64 = 0
        var handleType: Int32 = 0
        let status = base64.withCString { b64 -> UInt32 in
            if let mimeType {
                return mimeType.withCString { mime in
                    BamlApi.mediaFromBase64(kind.rawValue, b64, mime, &key, &handleType)
                }
            }
            return BamlApi.mediaFromBase64(kind.rawValue, b64, nil, &key, &handleType)
        }
        guard status == BAML_CFFI_STATUS_OK.rawValue else {
            throw BamlDecodeError.unsupported("media_from_base64 failed with status \(status)")
        }
        guard
            let wireType = BamlBridge_Cffi_V1_BamlHandleType(rawValue: Int(handleType))
        else {
            throw BamlDecodeError.unsupported("unknown media handle type \(handleType)")
        }
        return BamlHandle(key: key, handleType: wireType)
    }
}
