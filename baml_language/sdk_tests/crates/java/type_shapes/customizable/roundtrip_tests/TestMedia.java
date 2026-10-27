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
import static org.junit.jupiter.api.Assertions.fail;

import baml_bridge.BamlError;
import baml_bridge.Union2;
import baml_bridge.Union4;
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

    // --- media under a union ------------------------------------------------
    //
    // A union member that is media crosses as a media value, not a class or
    // scalar: it must decode into its own arm, never into `null`.

    @Test
    void test_media_round_trip_image_or_audio() {
        Image image = Fns.return_image(URL, null);
        Union2<Image, Audio> asImage =
                Fns.round_trip_image_or_audio(new Union2.Arm0<Image, Audio>(image));
        if (!(asImage instanceof Union2.Arm0<Image, Audio> imageArm)) {
            fail("expected the image arm, got " + asImage);
            return;
        }
        assertEquals(URL, imageArm.value().url());

        Audio audio = Fns.return_audio(URL, null);
        Union2<Image, Audio> asAudio =
                Fns.round_trip_image_or_audio(new Union2.Arm1<Image, Audio>(audio));
        if (!(asAudio instanceof Union2.Arm1<Image, Audio> audioArm)) {
            fail("expected the audio arm, got " + asAudio);
            return;
        }
        assertEquals(URL, audioArm.value().url());
    }

    @Test
    void test_media_round_trip_all_media() {
        Union4<Image, Audio, Video, Pdf> image = Fns.round_trip_all_media(
                new Union4.Arm0<Image, Audio, Video, Pdf>(Fns.return_image(URL, null)));
        if (!(image instanceof Union4.Arm0<Image, Audio, Video, Pdf> imageArm)) {
            fail("expected the image arm, got " + image);
            return;
        }
        assertEquals(URL, imageArm.value().url());

        Union4<Image, Audio, Video, Pdf> audio = Fns.round_trip_all_media(
                new Union4.Arm1<Image, Audio, Video, Pdf>(Fns.return_audio(URL, null)));
        if (!(audio instanceof Union4.Arm1<Image, Audio, Video, Pdf> audioArm)) {
            fail("expected the audio arm, got " + audio);
            return;
        }
        assertEquals(URL, audioArm.value().url());

        Union4<Image, Audio, Video, Pdf> video = Fns.round_trip_all_media(
                new Union4.Arm2<Image, Audio, Video, Pdf>(Fns.return_video(URL, null)));
        if (!(video instanceof Union4.Arm2<Image, Audio, Video, Pdf> videoArm)) {
            fail("expected the video arm, got " + video);
            return;
        }
        assertEquals(URL, videoArm.value().url());

        Union4<Image, Audio, Video, Pdf> pdf = Fns.round_trip_all_media(
                new Union4.Arm3<Image, Audio, Video, Pdf>(Fns.return_pdf(URL, null)));
        if (!(pdf instanceof Union4.Arm3<Image, Audio, Video, Pdf> pdfArm)) {
            fail("expected the pdf arm, got " + pdf);
            return;
        }
        assertEquals(URL, pdfArm.value().url());
    }

    @Test
    void test_media_round_trip_media_or_image_class() {
        // `image | Image`: the media type and a user class whose name collides
        // with it stay distinct arms.
        Union2<Image, baml_sdk.media.Image> media = Fns.round_trip_media_or_image_class(
                new Union2.Arm0<Image, baml_sdk.media.Image>(Fns.return_image(URL, null)));
        if (!(media instanceof Union2.Arm0<Image, baml_sdk.media.Image> mediaArm)) {
            fail("expected the media arm, got " + media);
            return;
        }
        assertEquals(URL, mediaArm.value().url());

        baml_sdk.media.Image labelled = new baml_sdk.media.Image("cover");
        Union2<Image, baml_sdk.media.Image> cls = Fns.round_trip_media_or_image_class(
                new Union2.Arm1<Image, baml_sdk.media.Image>(labelled));
        if (!(cls instanceof Union2.Arm1<Image, baml_sdk.media.Image> classArm)) {
            fail("expected the class arm, got " + cls);
            return;
        }
        assertEquals(labelled, classArm.value());
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
