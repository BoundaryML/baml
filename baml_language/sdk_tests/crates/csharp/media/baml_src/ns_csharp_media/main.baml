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

function round_trip_image(value: image) -> image {
  value
}

function round_trip_audio(value: audio) -> audio {
  value
}

function round_trip_video(value: video) -> video {
  value
}

function round_trip_pdf(value: pdf) -> pdf {
  value
}

class DynamicPayload {
  name string
  count int
}

function round_trip_payload(value: DynamicPayload) -> DynamicPayload {
  value
}

function round_trip_unknown(value: unknown) -> unknown {
  value
}
