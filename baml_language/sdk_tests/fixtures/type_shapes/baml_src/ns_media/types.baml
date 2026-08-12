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

class MediaShapes {
  optional_image image?
  images image[]
  optional_images image?[]
  images_by_name map<string, image>
  optional_list image[]?
}

// A nominal type whose projected component collides with the image primitive.
class Image {
  label string
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

function round_trip_optional_image(x: image?) -> image? {
  x
}

function round_trip_image_list(x: image[]) -> image[] {
  x
}

function round_trip_image_map(x: map<string, image>) -> map<string, image> {
  x
}

function round_trip_media_shapes(x: MediaShapes) -> MediaShapes {
  x
}

function round_trip_image_or_audio(x: image | audio) -> image | audio {
  x
}

function round_trip_media_or_image_class(x: image | Image) -> image | Image {
  x
}

// Four candidates exceed the default precise-union threshold. Go therefore
// projects this signature as `any` while preserving concrete media values at
// runtime.
function round_trip_all_media(x: image | audio | video | pdf) -> image | audio | video | pdf {
  x
}

// Generator-owned local names stay legal media parameters and retain their
// exact wire spellings.
function reserved_media_names(ctx: image, result: audio, err: video, bootstrap: pdf) -> image {
  ctx
}
