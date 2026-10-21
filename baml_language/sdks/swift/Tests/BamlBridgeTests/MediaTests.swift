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

    func testNamedContentOfTheGenericKindIsBuilt() throws {
        let handle = try BamlMedia.fromFileContent(
            .generic, file: "/nowhere/notes.bin", base64: "aGk=", mimeType: nil)
        let media = try XCTUnwrap(handle.portableMedia)
        XCTAssertEqual(media.media, .other)
        XCTAssertEqual(media.fileContent.name, "notes.bin")
        XCTAssertEqual(media.mimeType, "application/octet-stream")
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
