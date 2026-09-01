---
title: "media.Audio"
description: "Class media.Audio from the generated baml package reference."
---

An audio clip value. Construct with `Audio.from_url`, `Audio.from_file`, or `Audio.from_base64`.

```baml
class media.Audio
```

## Fields

### _data

```baml
_data: $rust_type
```

No description is available yet.

## Methods

### base64

```baml
function base64(self: baml.media.Audio) -> string
```

No description is available yet.

### file

```baml
function file(self: baml.media.Audio) -> string | null
```

No description is available yet.

### from_base64

```baml
function from_base64(base64: string, mime_type: string | null) -> audio
```

Creates an `Audio` value from a base64-encoded string. Optionally specify `mime_type`.

### from_file

```baml
function from_file(file: string, mime_type: string | null) -> audio
```

Creates an `Audio` value from a local file path. Optionally specify `mime_type`.

### from_url

```baml
function from_url(url: string, mime_type: string | null) -> audio
```

Creates an `Audio` value from a URL. Optionally specify `mime_type`.

### mime_type

```baml
function mime_type(self: baml.media.Audio) -> string | null
```

No description is available yet.

### url

```baml
function url(self: baml.media.Audio) -> string | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_media/media.baml:1534`_
