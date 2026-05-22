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

function ReturnImage(url: string, mime: string?) -> image {
  image.from_url(url, mime)
}

function ReturnAudio(url: string, mime: string?) -> audio {
  audio.from_url(url, mime)
}

function ReturnVideo(url: string, mime: string?) -> video {
  video.from_url(url, mime)
}

function ReturnPdf(url: string, mime: string?) -> pdf {
  pdf.from_url(url, mime)
}

function RoundTripImage(x: image) -> image {
  x
}

function RoundTripAudio(x: audio) -> audio {
  x
}

function RoundTripVideo(x: video) -> video {
  x
}

function RoundTripPdf(x: pdf) -> pdf {
  x
}

function RoundTripMedia(m: Media) -> Media {
  m
}
