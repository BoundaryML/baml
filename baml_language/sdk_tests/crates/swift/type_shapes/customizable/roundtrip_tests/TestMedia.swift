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

    func test_media_round_trip_all_media() throws {
        // Each media kind sent through the four-way media union comes back as
        // its own arm, carrying its URL.
        let image = try Baml.media.round_trip_all_media(
            x: .t0(try Baml.media.return_image(url: url, mime: nil)))
        guard case .t0(let imageArm) = image else {
            return XCTFail("expected the image arm, got \(image)")
        }
        XCTAssertEqual(try imageArm.url(), url)

        let audio = try Baml.media.round_trip_all_media(
            x: .t1(try Baml.media.return_audio(url: url, mime: nil)))
        guard case .t1(let audioArm) = audio else {
            return XCTFail("expected the audio arm, got \(audio)")
        }
        XCTAssertEqual(try audioArm.url(), url)

        let video = try Baml.media.round_trip_all_media(
            x: .t2(try Baml.media.return_video(url: url, mime: nil)))
        guard case .t2(let videoArm) = video else {
            return XCTFail("expected the video arm, got \(video)")
        }
        XCTAssertEqual(try videoArm.url(), url)

        let pdf = try Baml.media.round_trip_all_media(
            x: .t3(try Baml.media.return_pdf(url: url, mime: nil)))
        guard case .t3(let pdfArm) = pdf else {
            return XCTFail("expected the pdf arm, got \(pdf)")
        }
        XCTAssertEqual(try pdfArm.url(), url)
    }

    func test_media_round_trip_media_or_image_class() throws {
        // `image | Image`: the media type and the user class whose name
        // collides with it stay distinct arms.
        let media = try Baml.media.round_trip_media_or_image_class(
            x: .t0(try Baml.media.return_image(url: url, mime: nil)))
        guard case .t0(let mediaArm) = media else {
            return XCTFail("expected the media arm, got \(media)")
        }
        XCTAssertEqual(try mediaArm.url(), url)

        let labelled = Baml.media.Image(label: "cover")
        let cls = try Baml.media.round_trip_media_or_image_class(x: .t1(labelled))
        guard case .t1(let classArm) = cls else {
            return XCTFail("expected the class arm, got \(cls)")
        }
        XCTAssertEqual(classArm, labelled)
    }
}
