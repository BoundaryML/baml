/// A PDF document value. Construct with `Pdf.from_url`, `Pdf.from_file`, or `Pdf.from_base64`.
class Pdf {
  _data $rust_type

  /// Serializes this PDF to a JSON representation for passing to an LLM provider.
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns the URL of this PDF, or `null` if it was not constructed from a URL.
  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  /// Returns the file path of this PDF, or `null` if it was not constructed from a file path.
  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  /// Returns the base64-encoded content of this PDF.
  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  /// Returns the MIME type of this PDF, or `null` if not set.
  //baml:vm
  function mime_type(self) -> string? {
    $rust_function
  }

  /// Creates a `Pdf` value from a URL. Optionally specify `mime_type` (defaults to `"application/pdf"`).
  function from_url(url: string, mime_type: string?) -> pdf {
    $rust_function
  }

  /// Creates a `Pdf` value from a local file path. Optionally specify `mime_type`.
  function from_file(file: string, mime_type: string?) -> pdf {
    $rust_function
  }

  /// Creates a `Pdf` value from a base64-encoded string. Optionally specify `mime_type`.
  function from_base64(base64: string, mime_type: string?) -> pdf {
    $rust_function
  }
}

/// An audio clip value. Construct with `Audio.from_url`, `Audio.from_file`, or `Audio.from_base64`.
class Audio {
  _data $rust_type

  /// Serializes this audio clip to a JSON representation for passing to an LLM provider.
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns the URL of this audio clip, or `null` if it was not constructed from a URL.
  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  /// Returns the file path of this audio clip, or `null` if it was not constructed from a file path.
  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  /// Returns the base64-encoded content of this audio clip.
  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  /// Returns the MIME type of this audio clip, or `null` if not set.
  //baml:vm
  function mime_type(self) -> string? {
    $rust_function
  }

  /// Creates an `Audio` value from a URL. Optionally specify `mime_type`.
  function from_url(url: string, mime_type: string?) -> audio {
    $rust_function
  }

  /// Creates an `Audio` value from a local file path. Optionally specify `mime_type`.
  function from_file(file: string, mime_type: string?) -> audio {
    $rust_function
  }

  /// Creates an `Audio` value from a base64-encoded string. Optionally specify `mime_type`.
  function from_base64(base64: string, mime_type: string?) -> audio {
    $rust_function
  }
}

/// A video clip value. Construct with `Video.from_url`, `Video.from_file`, or `Video.from_base64`.
class Video {
  _data $rust_type

  /// Serializes this video clip to a JSON representation for passing to an LLM provider.
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns the URL of this video clip, or `null` if it was not constructed from a URL.
  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  /// Returns the file path of this video clip, or `null` if it was not constructed from a file path.
  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  /// Returns the base64-encoded content of this video clip.
  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  /// Returns the MIME type of this video clip, or `null` if not set.
  //baml:vm
  function mime_type(self) -> string? {
    $rust_function
  }

  /// Creates a `Video` value from a URL. Optionally specify `mime_type`.
  function from_url(url: string, mime_type: string?) -> video {
    $rust_function
  }

  /// Creates a `Video` value from a local file path. Optionally specify `mime_type`.
  function from_file(file: string, mime_type: string?) -> video {
    $rust_function
  }

  /// Creates a `Video` value from a base64-encoded string. Optionally specify `mime_type`.
  function from_base64(base64: string, mime_type: string?) -> video {
    $rust_function
  }
}

/// An image value. Construct with `Image.from_url`, `Image.from_file`, or `Image.from_base64`.
class Image {
  _data $rust_type

  /// Serializes this image to a JSON representation for passing to an LLM provider.
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns the URL of this image, or `null` if it was not constructed from a URL.
  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  /// Returns the file path of this image, or `null` if it was not constructed from a file path.
  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  /// Returns the base64-encoded content of this image.
  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  /// Returns the MIME type of this image, or `null` if not set.
  //baml:vm
  function mime_type(self) -> string? {
    $rust_function
  }

  /// Creates an `Image` value from a URL. Optionally specify `mime_type`.
  function from_url(url: string, mime_type: string?) -> image {
    $rust_function
  }

  /// Creates an `Image` value from a local file path. Optionally specify `mime_type`.
  function from_file(file: string, mime_type: string?) -> image {
    $rust_function
  }

  /// Creates an `Image` value from a base64-encoded string. Optionally specify `mime_type`.
  function from_base64(base64: string, mime_type: string?) -> image {
    $rust_function
  }
}
