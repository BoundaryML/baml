import Foundation
import XCTest

@testable import BamlBridge

/// A media value holds content, never a path. Content that was read from a
/// file keeps the base name of that file.
final class MediaTests: XCTestCase {
    func testNamedContentSurvivesThePortablePayload() throws {
        let handle = try BamlMedia.fromFileContent(
            .image, file: "/nowhere/cat.jpg", base64: "aGk=", mimeType: nil)
        let media = try XCTUnwrap(handle.portableMedia)
        XCTAssertEqual(media.fileContent.name, "cat.jpg")
        XCTAssertEqual(media.fileContent.base64, "aGk=")
        XCTAssertEqual(media.mimeType, "image/jpeg")
        let decoded = try XCTUnwrap(BamlMedia.decodePortable(media).portableMedia)
        XCTAssertEqual(decoded.fileContent.name, "cat.jpg")
        XCTAssertEqual(decoded.fileContent.base64, "aGk=")
    }

    /// A media value always has a kind; the wire's reserved or unknown
    /// kinds name none.
    func testPortableMediaWithoutAKindIsRefused() {
        for raw in [0, 5, 42] {
            var media = BamlBridge_Cffi_V1_BamlValueMedia()
            media.media = BamlBridge_Cffi_V1_MediaTypeEnum(rawValue: raw)!
            media.base64 = "aGk="
            XCTAssertThrowsError(try BamlMedia.decodePortable(media), "kind \(raw)")
        }
    }

    func testOnlyContentReadFromAFileHasAName() throws {
        let plain = try XCTUnwrap(
            BamlMedia.fromBase64(.image, "aGk=", mimeType: nil).portableMedia)
        XCTAssertEqual(plain.base64, "aGk=")
        XCTAssertNil(plain.value.flatMap { value -> String? in
            if case .fileContent(let content) = value { return content.name }
            return nil
        })
    }
}
