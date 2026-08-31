/// A PDF document value.
///
/// May be backed by a URL, local file path, or base64-encoded content.
class Pdf {
    _data: $rust_type,

    /// Returns the URL of this PDF, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the file path of this PDF, or `null` if it was not constructed from a file path.
    //baml:vm
    function file(self) -> string? throws never {
        $rust_function
    }

    /// Returns the base64-encoded content of this PDF.
    //baml:vm
    function base64(self) -> string throws never {
        $rust_function
    }

    /// Returns the MIME type of this PDF, or `null` if not set.
    //baml:vm
    function mime_type(self) -> string? throws never {
        $rust_function
    }

    /// Creates a `Pdf` value from a URL. Optionally specify `mime_type` (defaults to `"application/pdf"`).
    function from_url(url: string, mime_type: string?) -> pdf throws never {
        $rust_function
    }

    /// Creates a `Pdf` value from a local file path. Optionally specify `mime_type`.
    function from_file(file: string, mime_type: string?) -> pdf throws never {
        $rust_function
    }

    /// Creates a `Pdf` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> pdf throws never {
        $rust_function
    }
}

/// An audio clip value.
///
/// May be backed by a URL, local file path, or base64-encoded content.
class Audio {
    _data: $rust_type,

    /// Returns the URL of this audio clip, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the file path of this audio clip, or `null` if it was not constructed from a file path.
    //baml:vm
    function file(self) -> string? throws never {
        $rust_function
    }

    /// Returns the base64-encoded content of this audio clip.
    //baml:vm
    function base64(self) -> string throws never {
        $rust_function
    }

    /// Returns the MIME type of this audio clip, or `null` if not set.
    //baml:vm
    function mime_type(self) -> string? throws never {
        $rust_function
    }

    /// Creates an `Audio` value from a URL. Optionally specify `mime_type`.
    function from_url(url: string, mime_type: string?) -> audio throws never {
        $rust_function
    }

    /// Creates an `Audio` value from a local file path. Optionally specify `mime_type`.
    function from_file(file: string, mime_type: string?) -> audio throws never {
        $rust_function
    }

    /// Creates an `Audio` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> audio throws never {
        $rust_function
    }
}

/// A video clip value.
///
/// May be backed by a URL, local file path, or base64-encoded content.
class Video {
    _data: $rust_type,

    /// Returns the URL of this video clip, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the file path of this video clip, or `null` if it was not constructed from a file path.
    //baml:vm
    function file(self) -> string? throws never {
        $rust_function
    }

    /// Returns the base64-encoded content of this video clip.
    //baml:vm
    function base64(self) -> string throws never {
        $rust_function
    }

    /// Returns the MIME type of this video clip, or `null` if not set.
    //baml:vm
    function mime_type(self) -> string? throws never {
        $rust_function
    }

    /// Creates a `Video` value from a URL. Optionally specify `mime_type`.
    function from_url(url: string, mime_type: string?) -> video throws never {
        $rust_function
    }

    /// Creates a `Video` value from a local file path. Optionally specify `mime_type`.
    function from_file(file: string, mime_type: string?) -> video throws never {
        $rust_function
    }

    /// Creates a `Video` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> video throws never {
        $rust_function
    }
}

/// An image value.
///
/// May be backed by a URL, local file path, or base64-encoded content.
class Image {
    _data: $rust_type,

    /// Returns the URL of this image, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the file path of this image, or `null` if it was not constructed from a file path.
    //baml:vm
    function file(self) -> string? throws never {
        $rust_function
    }

    /// Returns the base64-encoded content of this image.
    //baml:vm
    function base64(self) -> string throws never {
        $rust_function
    }

    /// Returns the MIME type of this image, or `null` if not set.
    //baml:vm
    function mime_type(self) -> string? throws never {
        $rust_function
    }

    /// Creates an `Image` value from a URL. Optionally specify `mime_type`.
    function from_url(url: string, mime_type: string?) -> image throws never {
        $rust_function
    }

    /// Creates an `Image` value from a local file path. Optionally specify `mime_type`.
    function from_file(file: string, mime_type: string?) -> image throws never {
        $rust_function
    }

    /// Creates an `Image` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> image throws never {
        $rust_function
    }
}
