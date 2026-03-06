class Pdf {
  _data $rust_type

  function url(self) -> string? {
    $rust_function
  }

  function file(self) -> string? {
    $rust_function
  }

  function base64(self) -> string {
    $rust_function
  }

  function mime_type(self) -> string? {
    $rust_function
  }

  function from_url(url: string, mime_type: string?) -> pdf {
    $rust_function
  }

  function from_file(file: string, mime_type: string?) -> pdf {
    $rust_function
  }

  function from_base64(base64: string, mime_type: string?) -> pdf {
    $rust_function
  }
}

class Audio {
  _data $rust_type

  function url(self) -> string? {
    $rust_function
  }

  function file(self) -> string? {
    $rust_function
  }

  function base64(self) -> string {
    $rust_function
  }

  function mime_type(self) -> string? {
    $rust_function
  }

  function from_url(url: string, mime_type: string?) -> audio {
    $rust_function
  }

  function from_file(file: string, mime_type: string?) -> audio {
    $rust_function
  }

  function from_base64(base64: string, mime_type: string?) -> audio {
    $rust_function
  }
}

class Video {
  _data $rust_type

  function url(self) -> string? {
    $rust_function
  }

  function file(self) -> string? {
    $rust_function
  }

  function base64(self) -> string {
    $rust_function
  }

  function mime_type(self) -> string? {
    $rust_function
  }

  function from_url(url: string, mime_type: string?) -> video {
    $rust_function
  }

  function from_file(file: string, mime_type: string?) -> video {
    $rust_function
  }

  function from_base64(base64: string, mime_type: string?) -> video {
    $rust_function
  }
}

class Image {
  _data $rust_type

  function url(self) -> string? {
    $rust_function
  }

  function file(self) -> string? {
    $rust_function
  }

  function base64(self) -> string {
    $rust_function
  }

  function mime_type(self) -> string? {
    $rust_function
  }

  function from_url(url: string, mime_type: string?) -> image {
    $rust_function
  }

  function from_file(file: string, mime_type: string?) -> image {
    $rust_function
  }

  function from_base64(base64: string, mime_type: string?) -> image {
    $rust_function
  }
}
