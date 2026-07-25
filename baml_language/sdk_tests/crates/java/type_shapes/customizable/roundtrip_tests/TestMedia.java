// Roundtrip coverage for `baml_sdk.media`.
//
// Media values can't be hand-built from raw fields — there is no
// wire-visible constructor for a handle-backed class — so, same as Python,
// each value is sourced from the matching `return_*` function (which builds
// it engine-side via `image.from_url(...)` etc.) and only presence /
// round-trip is asserted, not field equality.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_media.py — same test names, cases, inputs, assertions.
//
// java-port note: `Image`/`Audio`/`Video`/`Pdf` are runtime-owned
// handle-backed classes per the conventions doc ("Media" row), re-exported
// under `baml_sdk.baml.media` rather than code-generated per fixture.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertNotNull;

import baml_sdk.baml.media.Audio;
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;
import baml_sdk.baml.media.Video;
import baml_sdk.media.Fns;
import baml_sdk.media.Media;
import org.junit.jupiter.api.Test;

class TestMedia {

    private static final String URL = "https://example.com/asset";

    // --- decode path (return_*) works ---------------------------------------

    @Test
    void test_media_return_image() {
        assertNotNull(Fns.return_image(URL, null));
    }

    @Test
    void test_media_return_audio() {
        assertNotNull(Fns.return_audio(URL, null));
    }

    @Test
    void test_media_return_video() {
        assertNotNull(Fns.return_video(URL, null));
    }

    @Test
    void test_media_return_pdf() {
        assertNotNull(Fns.return_pdf(URL, null));
    }

    // --- encode path (round_trip_*) -----------------------------------------

    @Test
    void test_media_round_trip_image() {
        Image img = Fns.return_image(URL, null);
        assertNotNull(Fns.round_trip_image(img));
    }

    @Test
    void test_media_round_trip_audio() {
        Audio aud = Fns.return_audio(URL, null);
        assertNotNull(Fns.round_trip_audio(aud));
    }

    @Test
    void test_media_round_trip_video() {
        Video vid = Fns.return_video(URL, null);
        assertNotNull(Fns.round_trip_video(vid));
    }

    @Test
    void test_media_round_trip_pdf() {
        Pdf pdf = Fns.return_pdf(URL, null);
        assertNotNull(Fns.round_trip_pdf(pdf));
    }

    @Test
    void test_media_round_trip_media() {
        Media m =
                new Media(
                        Fns.return_image(URL, null),
                        Fns.return_audio(URL, null),
                        Fns.return_video(URL, null),
                        Fns.return_pdf(URL, null));
        assertNotNull(Fns.round_trip_media(m));
    }
}
