import { b } from "./test-setup";
import { Image, Audio, Pdf, Video } from "@boundaryml/baml";
import { image_b64, audio_b64, pdf_b64, video_b64 } from "./base64_test_data";

describe("Media Tests", () => {
  it("should work with image from url", async () => {
    let res = await b.TestImageInput(
      Image.fromUrl(
        "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
      ),
    );
    expect(res.toLowerCase()).toMatch(/(green|yellow|ogre|shrek)/);
  });

  it("should work with image from base 64", async () => {
    let res = await b.TestImageInput(Image.fromBase64("image/png", image_b64));
    expect(res.toLowerCase()).toMatch(/(green|yellow|ogre|shrek)/);
  });

  it("should work with audio base 64", async () => {
    let res = await b.AudioInput(Audio.fromBase64("audio/mp3", audio_b64));
    expect(res.toLowerCase()).toContain("yes");
  });

  it("should work with audio from url", async () => {
    let res = await b.AudioInput(
      Audio.fromUrl(
        "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg",
      ),
    );

    expect(res.toLowerCase()).toContain("no");
  });

  it("should work with pdf from url", async () => {
    let res = await b.PdfInput(
      Pdf.fromUrl(
        "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
        "application/pdf"
      ),
    );
    expect(res.toLowerCase()).toMatch(/(dummy|pdf|document)/);
  });

  it("should work with pdf from base 64", async () => {
    let res = await b.PdfInput(Pdf.fromBase64("application/pdf", pdf_b64));
    expect(res.toLowerCase()).toMatch(/(bookmarks|pdf|sample|usage)/);
  });

  it("should work with video and a youtube url for gemini", async () => {
    // This test uses a public YouTube video URL as input.
    // See: https://youtu.be/dQw4w9WgXcQ?si=aQdfsK0DdcDtCCud
    let res = await b.VideoInputGemini(
      Video.fromUrl("https://youtu.be/dQw4w9WgXcQ?si=aQdfsK0DdcDtCCud")
    );
    expect(res.toLowerCase()).toMatch(/(singing|rickroll|dancing)/);
  });

  it("should work with video from base 64", async () => {
    let res = await b.VideoInputGemini(Video.fromBase64("video/mp4", video_b64));
    expect(res.toLowerCase()).toMatch(/(cartoon|sky|field)/);
  });
});
