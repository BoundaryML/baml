// Media Ty variants — one row per `Primitive(Image|Audio|Video|Pdf)` from 18a.
//
// `Media` exposes one field per variant so the type rendering ends up in the
// generated Python. The matching `Return*` functions exercise the BAML
// frontend for the same variants but aren't emitted into Python (see 18b).

class Media {
  image_field image
  audio_field audio
  video_field video
  pdf_field pdf
}

function return_image(url: string, mime: string?) -> image {
  image.from_url(url, mime)
}

function return_audio(url: string, mime: string?) -> audio {
  audio.from_url(url, mime)
}

function return_video(url: string, mime: string?) -> video {
  video.from_url(url, mime)
}

function return_pdf(url: string, mime: string?) -> pdf {
  pdf.from_url(url, mime)
}

function round_trip_image(x: image) -> image {
  x
}

function round_trip_audio(x: audio) -> audio {
  x
}

function round_trip_video(x: video) -> video {
  x
}

function round_trip_pdf(x: pdf) -> pdf {
  x
}

function round_trip_media(m: Media) -> Media {
  m
}
