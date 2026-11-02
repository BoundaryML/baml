# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")
require "tempfile"

class HandlesTest < Minitest::Test
  PNG_B64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk" \
    "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC"

  def test_handles_image_from_base64_roundtrips_payload
    payload = PNG_B64.dup
    image = Baml::Image.from_base64(payload, mime_type: "image/png")
    payload.replace("changed")
    assert image.frozen?
    assert image.base64.frozen?
    assert_equal "image/png", image.mime_type
    assert_equal PNG_B64, image.base64
    result = BamlSdk::Media.round_trip_image(image)
    assert_equal PNG_B64, result.base64
    assert_equal "image/png", BamlSdk::Media.image_mime_type(image)
    assert_nil result.url
    assert_nil result.name

    # File values own their contents, not a path or an engine capability.
    Tempfile.create(["scan", ".pdf"]) do |file|
      file.write("%PDF-1.7")
      file.close
      pdf = Baml::Pdf.from_file(file.path)
      File.unlink(file.path)
      assert_equal "JVBERi0xLjc=", pdf.base64
      assert_equal File.basename(file.path), pdf.name
      assert_nil pdf.mime_type
      decoded = BamlSdk::Media.round_trip_pdf(pdf)
      assert_equal pdf.name, decoded.name
      assert_equal pdf.base64, decoded.base64
      assert_equal "application/pdf", decoded.mime_type
      assert_raises(Errno::ENOENT) { Baml::Pdf.from_file(file.path) }
    end
    audio = Baml::Audio.from_file_content("recording.webm", "YQ==")
    assert_nil audio.mime_type
    assert_equal "recording.webm", BamlSdk::Media.round_trip_audio(audio).name
    video = Baml::Video.from_url("https://example.com/video.mp4", mime_type: "video/mp4")
    assert_equal video.mime_type, BamlSdk::Media.round_trip_video(video).mime_type
  end

  def test_handles_open_file_returns_file_handle
    Tempfile.create("digits") do |file|
      file.write("0123456789")
      file.flush
      value = BamlSdk::Baml::Fs.open(file.path, "r")
      assert_instance_of BamlSdk::Baml::Fs::File, value
      assert_nil value.close
    end
  end

  def test_handles_file_cursor_state_persists_across_calls
    Tempfile.create("digits") do |file|
      file.write("0123456789")
      file.flush
      value = BamlSdk::Baml::Fs.open(file.path, "r")
      begin
        assert_equal 3, value.seek_from("current", 3)
        assert_equal 6, value.seek_from("current", 3)
        assert_equal 0, value.seek_from("start", 0)
        assert_equal 2, value.seek_from("current", 2)
        assert_equal "23456789", value.text
      ensure
        assert_nil value.close
      end
    end
  end
end
