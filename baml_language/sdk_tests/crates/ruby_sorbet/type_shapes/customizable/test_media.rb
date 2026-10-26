# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MediaTest < Minitest::Test
  URL = "https://example.com/asset"

  def test_media_return_image
    image = BamlSdk::Media.return_image(URL, nil)
    assert_instance_of Baml::Image, image
    assert_equal URL, image.url
  end

  def test_media_return_audio
    audio = BamlSdk::Media.return_audio(URL, nil)
    assert_instance_of Baml::Audio, audio
    assert_equal URL, audio.url
  end

  def test_media_return_video
    video = BamlSdk::Media.return_video(URL, nil)
    assert_instance_of Baml::Video, video
    assert_equal URL, video.url
  end

  def test_media_return_pdf
    pdf = BamlSdk::Media.return_pdf(URL, nil)
    assert_instance_of Baml::Pdf, pdf
    assert_equal URL, pdf.url
  end

  def test_media_round_trip_image
    image = BamlSdk::Media.return_image(URL, nil)
    assert_equal URL, BamlSdk::Media.round_trip_image(image).url
  end

  def test_media_round_trip_audio
    audio = BamlSdk::Media.return_audio(URL, nil)
    assert_equal URL, BamlSdk::Media.round_trip_audio(audio).url
  end

  def test_media_round_trip_video
    video = BamlSdk::Media.return_video(URL, nil)
    assert_equal URL, BamlSdk::Media.round_trip_video(video).url
  end

  def test_media_round_trip_pdf
    pdf = BamlSdk::Media.return_pdf(URL, nil)
    assert_equal URL, BamlSdk::Media.round_trip_pdf(pdf).url
  end

  def test_media_round_trip_media
    media = BamlSdk::Media::Media.new(
      image_field: BamlSdk::Media.return_image(URL, nil),
      audio_field: BamlSdk::Media.return_audio(URL, nil),
      video_field: BamlSdk::Media.return_video(URL, nil),
      pdf_field: BamlSdk::Media.return_pdf(URL, nil)
    )
    result = BamlSdk::Media.round_trip_media(media)
    %i[image_field audio_field video_field pdf_field].each do |field|
      assert_equal URL, result.public_send(field).url
    end
  end
end
