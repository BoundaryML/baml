// Roundtrip coverage for `baml_sdk.media`.
//
// Media wrappers are native-backed inside Java, while bridge traffic uses the
// canonical portable kind + URL/base64 representation. These tests verify
// both wrapper reification and preservation of the portable URL payload.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_media.py — same core cases and inputs, with payload assertions.
//
// java-port note: `Image`/`Audio`/`Video`/`Pdf` are runtime-owned
// handle-backed classes per the conventions doc ("Media" row), re-exported
// under `baml_sdk.baml.media` rather than code-generated per fixture.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import baml_bridge.BamlError;
import baml_sdk.baml.media.Audio;
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;
import baml_sdk.baml.media.Video;
import baml_sdk.media.Fns;
import baml_sdk.media.Media;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;

class TestMedia {

    private static final String URL = "https://example.com/asset";

    // --- decode path (return_*) works ---------------------------------------

    @Test
    void test_media_return_image() {
        Image image = Fns.return_image(URL, null);
        assertNotNull(image);
        assertEquals(URL, image.url());
    }

    @Test
    void test_media_return_audio() {
        Audio audio = Fns.return_audio(URL, null);
        assertNotNull(audio);
        assertEquals(URL, audio.url());
    }

    @Test
    void test_media_return_video() {
        Video video = Fns.return_video(URL, null);
        assertNotNull(video);
        assertEquals(URL, video.url());
    }

    @Test
    void test_media_return_pdf() {
        Pdf pdf = Fns.return_pdf(URL, null);
        assertNotNull(pdf);
        assertEquals(URL, pdf.url());
    }

    // --- encode path (round_trip_*) -----------------------------------------

    @Test
    void test_media_round_trip_image() {
        Image img = Fns.return_image(URL, null);
        assertEquals(URL, Fns.round_trip_image(img).url());
    }

    @Test
    void test_media_round_trip_audio() {
        Audio aud = Fns.return_audio(URL, null);
        assertEquals(URL, Fns.round_trip_audio(aud).url());
    }

    @Test
    void test_media_round_trip_video() {
        Video vid = Fns.return_video(URL, null);
        assertEquals(URL, Fns.round_trip_video(vid).url());
    }

    @Test
    void test_media_round_trip_pdf() {
        Pdf pdf = Fns.return_pdf(URL, null);
        assertEquals(URL, Fns.round_trip_pdf(pdf).url());
    }

    @Test
    void test_media_round_trip_media() {
        Media m =
                new Media(
                        Fns.return_image(URL, null),
                        Fns.return_audio(URL, null),
                        Fns.return_video(URL, null),
                        Fns.return_pdf(URL, null));
        Media roundTripped = Fns.round_trip_media(m);
        assertEquals(URL, roundTripped.image_field().url());
        assertEquals(URL, roundTripped.audio_field().url());
        assertEquals(URL, roundTripped.video_field().url());
        assertEquals(URL, roundTripped.pdf_field().url());
    }

    // --- from_file: BAML's from_file, which reads the file now ---------------

    // SDK_PARITY_LINT(skip): validates the Java media wrapper's from_file, which is BAML's from_file
    @Test
    void test_media_from_file_reads_now_and_keeps_the_base_name() throws Exception {
        Path path = Files.createTempFile("baml-media-", ".pdf");
        Files.write(path, "%PDF-1.7".getBytes(StandardCharsets.UTF_8));
        Pdf pdf = Pdf.from_file(path.toString());
        // The file can go: the value holds its content.
        Files.delete(path);
        assertEquals("JVBERi0xLjc=", pdf.base64());
        assertEquals(path.getFileName().toString(), pdf.name());
        assertEquals("application/pdf", pdf.mime_type());
        assertNull(pdf.url());
        // The name crosses the call boundary both ways.
        assertEquals(pdf.name(), Fns.round_trip_pdf(pdf).name());
        // A file that cannot be read fails when the value is built, with the
        // error BAML code would get.
        assertThrows(BamlError.class, () -> Pdf.from_file(path.toString()));
    }
}
