/// A PDF document value.
///
/// Backed by a URL or by base64-encoded content. A file is read when the
/// value is built from it: the value holds its content, never its path.
class Pdf {
    _data: $rust_type,

    /// Returns the URL of this PDF, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the name of this PDF, or `null` if it has none. A PDF built from a
    /// file is named by the file's base name.
    //baml:vm
    function name(self) -> string? throws never {
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

    /// Creates a `Pdf` value from the content of a local file, which is
    /// read now. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    ///
    /// # Throws
    ///
    /// - `Io` if the file cannot be opened or read.
    /// - `InvalidArgument` only as `baml.fs.open` declares it: a path that
    ///   cannot be opened is `Io`.
    function from_file(
        file: string,
        mime_type: string?,
    ) -> pdf throws root.errors.Io | root.errors.InvalidArgument {
        let handle = root.fs.open(file, "r");
        defer { handle.close() }
        Pdf.from_file_content(file, handle.bytes().to_base64(), mime_type)
    }

    /// Creates a `Pdf` value from base64-encoded content that was read
    /// from `file`. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    function from_file_content(
        file: string,
        base64: string,
        mime_type: string?,
    ) -> pdf throws never {
        $rust_function
    }

    /// Returns the MIME type that `source`, a file name or URL, implies for
    /// a PDF: by its extension, or the usual type for a PDF when the
    /// extension says nothing. A URL's query and fragment are not part of
    /// its extension.
    function infer_mime_type(source: string) -> string throws never {
        $rust_function
    }

    /// Creates a `Pdf` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> pdf throws never {
        $rust_function
    }
}

/// An audio clip value.
///
/// Backed by a URL or by base64-encoded content. A file is read when the
/// value is built from it: the value holds its content, never its path.
class Audio {
    _data: $rust_type,

    /// Returns the URL of this audio clip, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the name of this audio clip, or `null` if it has none. An audio clip built from a
    /// file is named by the file's base name.
    //baml:vm
    function name(self) -> string? throws never {
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

    /// Creates an `Audio` value from the content of a local file, which is
    /// read now. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    ///
    /// # Throws
    ///
    /// - `Io` if the file cannot be opened or read.
    /// - `InvalidArgument` only as `baml.fs.open` declares it: a path that
    ///   cannot be opened is `Io`.
    function from_file(
        file: string,
        mime_type: string?,
    ) -> audio throws root.errors.Io | root.errors.InvalidArgument {
        let handle = root.fs.open(file, "r");
        defer { handle.close() }
        Audio.from_file_content(file, handle.bytes().to_base64(), mime_type)
    }

    /// Creates an `Audio` value from base64-encoded content that was read
    /// from `file`. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    function from_file_content(
        file: string,
        base64: string,
        mime_type: string?,
    ) -> audio throws never {
        $rust_function
    }

    /// Returns the MIME type that `source`, a file name or URL, implies for
    /// an audio clip: by its extension, or the usual type for an audio clip when the
    /// extension says nothing. A URL's query and fragment are not part of
    /// its extension.
    function infer_mime_type(source: string) -> string throws never {
        $rust_function
    }

    /// Creates an `Audio` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> audio throws never {
        $rust_function
    }
}

/// A video clip value.
///
/// Backed by a URL or by base64-encoded content. A file is read when the
/// value is built from it: the value holds its content, never its path.
class Video {
    _data: $rust_type,

    /// Returns the URL of this video clip, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the name of this video clip, or `null` if it has none. A video clip built from a
    /// file is named by the file's base name.
    //baml:vm
    function name(self) -> string? throws never {
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

    /// Creates a `Video` value from the content of a local file, which is
    /// read now. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    ///
    /// # Throws
    ///
    /// - `Io` if the file cannot be opened or read.
    /// - `InvalidArgument` only as `baml.fs.open` declares it: a path that
    ///   cannot be opened is `Io`.
    function from_file(
        file: string,
        mime_type: string?,
    ) -> video throws root.errors.Io | root.errors.InvalidArgument {
        let handle = root.fs.open(file, "r");
        defer { handle.close() }
        Video.from_file_content(file, handle.bytes().to_base64(), mime_type)
    }

    /// Creates a `Video` value from base64-encoded content that was read
    /// from `file`. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    function from_file_content(
        file: string,
        base64: string,
        mime_type: string?,
    ) -> video throws never {
        $rust_function
    }

    /// Returns the MIME type that `source`, a file name or URL, implies for
    /// a video clip: by its extension, or the usual type for a video clip when the
    /// extension says nothing. A URL's query and fragment are not part of
    /// its extension.
    function infer_mime_type(source: string) -> string throws never {
        $rust_function
    }

    /// Creates a `Video` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> video throws never {
        $rust_function
    }
}

/// An image value.
///
/// Backed by a URL or by base64-encoded content. A file is read when the
/// value is built from it: the value holds its content, never its path.
class Image {
    _data: $rust_type,

    /// Returns the URL of this image, or `null` if it was not constructed from a URL.
    //baml:vm
    function url(self) -> string? throws never {
        $rust_function
    }

    /// Returns the name of this image, or `null` if it has none. An image built from a
    /// file is named by the file's base name.
    //baml:vm
    function name(self) -> string? throws never {
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

    /// Creates an `Image` value from the content of a local file, which is
    /// read now. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    ///
    /// # Throws
    ///
    /// - `Io` if the file cannot be opened or read.
    /// - `InvalidArgument` only as `baml.fs.open` declares it: a path that
    ///   cannot be opened is `Io`.
    function from_file(
        file: string,
        mime_type: string?,
    ) -> image throws root.errors.Io | root.errors.InvalidArgument {
        let handle = root.fs.open(file, "r");
        defer { handle.close() }
        Image.from_file_content(file, handle.bytes().to_base64(), mime_type)
    }

    /// Creates an `Image` value from base64-encoded content that was read
    /// from `file`. The value is named by the file's base name. Optionally
    /// specify `mime_type`; otherwise it is inferred from the file's name.
    function from_file_content(
        file: string,
        base64: string,
        mime_type: string?,
    ) -> image throws never {
        $rust_function
    }

    /// Returns the MIME type that `source`, a file name or URL, implies for
    /// an image: by its extension, or the usual type for an image when the
    /// extension says nothing. A URL's query and fragment are not part of
    /// its extension.
    function infer_mime_type(source: string) -> string throws never {
        $rust_function
    }

    /// Creates an `Image` value from a base64-encoded string. Optionally specify `mime_type`.
    function from_base64(base64: string, mime_type: string?) -> image throws never {
        $rust_function
    }
}
