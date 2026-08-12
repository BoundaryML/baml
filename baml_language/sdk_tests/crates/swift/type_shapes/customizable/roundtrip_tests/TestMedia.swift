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
}
