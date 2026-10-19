// Roundtrip coverage for `Baml.media` — port of python_pydantic2
// `roundtrip_tests/test_media.py`. Media values are engine-minted
// (`X.from_url` inside the fixture); Python asserts non-None, Swift's
// analog is a successful typed call.
import XCTest
import Baml
import BamlBridge

private let url = "https://example.com/asset"

final class TestMedia: XCTestCase {
    func test_media_return_image() throws {
        _ = try Baml.media.return_image(url: url, mime: nil)
    }

    func test_media_return_audio() throws {
        _ = try Baml.media.return_audio(url: url, mime: nil)
    }

    func test_media_return_video() throws {
        _ = try Baml.media.return_video(url: url, mime: nil)
    }

    func test_media_return_pdf() throws {
        _ = try Baml.media.return_pdf(url: url, mime: nil)
    }

    func test_media_round_trip_image() throws {
        let x = try Baml.media.return_image(url: url, mime: nil)
        _ = try Baml.media.round_trip_image(x: x)
    }

    func test_media_round_trip_audio() throws {
        let x = try Baml.media.return_audio(url: url, mime: nil)
        _ = try Baml.media.round_trip_audio(x: x)
    }

    func test_media_round_trip_video() throws {
        let x = try Baml.media.return_video(url: url, mime: nil)
        _ = try Baml.media.round_trip_video(x: x)
    }

    func test_media_round_trip_pdf() throws {
        let x = try Baml.media.return_pdf(url: url, mime: nil)
        _ = try Baml.media.round_trip_pdf(x: x)
    }

    func test_media_round_trip_media() throws {
        let m = Baml.media.Media(
            image_field: try Baml.media.return_image(url: url, mime: nil),
            audio_field: try Baml.media.return_audio(url: url, mime: nil),
            video_field: try Baml.media.return_video(url: url, mime: nil),
            pdf_field: try Baml.media.return_pdf(url: url, mime: nil)
        )
        _ = try Baml.media.round_trip_media(m: m)
    }

    // Media is the type `image`, never the stdlib class wrapping it: every
    // container holding media states `image[]`, `map<string, image>`, ...

    func test_media_round_trip_image_list() throws {
        let x = try Baml.media.return_image(url: url, mime: nil)
        let result = try Baml.media.round_trip_image_list(x: [x, x])
        XCTAssertEqual(result.count, 2)
        XCTAssertEqual(try result[1].url(), url)
        XCTAssertEqual(try Baml.media.round_trip_image_list(x: []).count, 0)
    }

    func test_media_round_trip_optional_image_list() throws {
        let x = try Baml.media.return_image(url: url, mime: nil)
        XCTAssertEqual(try Baml.media.round_trip_optional_image_list(x: [x])?.count, 1)
        XCTAssertEqual(try Baml.media.round_trip_optional_image_list(x: [])?.count, 0)
        XCTAssertNil(try Baml.media.round_trip_optional_image_list(x: nil))
    }

    func test_media_round_trip_image_map() throws {
        let x = try Baml.media.return_image(url: url, mime: nil)
        let result = try Baml.media.round_trip_image_map(x: ["a": x])
        XCTAssertEqual(try result["a"]?.url(), url)
    }

    func test_media_round_trip_media_shapes() throws {
        let x = try Baml.media.return_image(url: url, mime: nil)
        let shapes = Baml.media.MediaShapes(
            optional_image: nil,
            images: [x],
            optional_images: [x, nil],
            images_by_name: ["a": x],
            optional_list: []
        )
        let result = try Baml.media.round_trip_media_shapes(x: shapes)
        XCTAssertEqual(result.images.count, 1)
        XCTAssertEqual(result.optional_images.count, 2)
        XCTAssertNil(result.optional_images[1])
        XCTAssertEqual(result.optional_list?.count, 0)
    }

    func test_media_round_trip_image_or_audio() throws {
        let x = try Baml.media.return_audio(url: url, mime: nil)
        let result = try Baml.media.round_trip_image_or_audio(x: .t1(x))
        XCTAssertEqual(try result.t1?.url(), url)
    }
}
