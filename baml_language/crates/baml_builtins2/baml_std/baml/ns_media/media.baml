class Pdf {
  _data $rust_type

  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  //baml:vm
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

  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  //baml:vm
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

  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  //baml:vm
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

  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  //baml:vm
  function url(self) -> string? {
    $rust_function
  }

  //baml:vm
  function file(self) -> string? {
    $rust_function
  }

  //baml:vm
  function base64(self) -> string {
    $rust_function
  }

  //baml:vm
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
