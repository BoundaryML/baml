# Type Serialization Test Specification

This file defines test cases for BAML type serialization across all target languages.
Each test specifies the BAML source, target path, and expected type strings for each language.

## Running Tests

Run these commands from the `engine/` directory:

```bash
# Run all type serialization tests for all languages
cargo test --lib type_gen

# Run tests for a specific language
cargo test -p generators-python --lib type_gen    # Python
cargo test -p generators-typescript --lib type_gen # TypeScript
cargo test -p generators-go --lib type_gen         # Go

# Run a specific test by name
cargo test -p generators-python --lib type_gen::check_on_optional
```

## Format

- Short types: `- Non-streaming: <type>` and `- Streaming: <type>`
- Long types: Use code blocks with the language identifier

---

# Primitives

## string_field

```baml
class T { f string }
```

### target: `T.f`

### Python

- Non-streaming: `str`
- Streaming: `typing.Optional[str]`

### TypeScript

- Non-streaming: `string`
- Streaming: `string | null`

### Go

- Non-streaming: `string`
- Streaming: `*string`

### Rust

- Non-streaming: `String`
- Streaming: `Option<String>`

---

## int_field

```baml
class T { f int }
```

### target: `T.f`

### Python

- Non-streaming: `int`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number`
- Streaming: `number | null`

### Go

- Non-streaming: `int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `i64`
- Streaming: `Option<i64>`

---

## float_field

```baml
class T { f float }
```

### target: `T.f`

### Python

- Non-streaming: `float`
- Streaming: `typing.Optional[float]`

### TypeScript

- Non-streaming: `number`
- Streaming: `number | null`

### Go

- Non-streaming: `float64`
- Streaming: `*float64`

### Rust

- Non-streaming: `f64`
- Streaming: `Option<f64>`

---

## bool_field

```baml
class T { f bool }
```

### target: `T.f`

### Python

- Non-streaming: `bool`
- Streaming: `typing.Optional[bool]`

### TypeScript

- Non-streaming: `boolean`
- Streaming: `boolean | null`

### Go

- Non-streaming: `bool`
- Streaming: `*bool`

### Rust

- Non-streaming: `bool`
- Streaming: `Option<bool>`

---

# Media Types

## image_field

```baml
class T { f image }
```

### target: `T.f`

### Python

- Non-streaming: `baml_py.Image`
- Streaming: `typing.Optional[baml_py.Image]`

### TypeScript

- Non-streaming: `Image`
- Streaming: `Image | null`

### Go

- Non-streaming: `Image`
- Streaming: `*types.Image`

### Rust

- Non-streaming: `Image`
- Streaming: `Option<types::Image>`

---

## optional_image

```baml
class T { f image? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[baml_py.Image]`
- Streaming: `typing.Optional[baml_py.Image]`

### TypeScript

- Non-streaming: `Image | null`
- Streaming: `Image | null`

### Go

- Non-streaming: `*Image`
- Streaming: `*types.Image`

### Rust

- Non-streaming: `Option<Image>`
- Streaming: `Option<types::Image>`

---

## audio_field

```baml
class T { f audio }
```

### target: `T.f`

### Python

- Non-streaming: `baml_py.Audio`
- Streaming: `typing.Optional[baml_py.Audio]`

### TypeScript

- Non-streaming: `Audio`
- Streaming: `Audio | null`

### Go

- Non-streaming: `Audio`
- Streaming: `*types.Audio`

### Rust

- Non-streaming: `Audio`
- Streaming: `Option<types::Audio>`

---

## optional_audio

```baml
class T { f audio? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[baml_py.Audio]`
- Streaming: `typing.Optional[baml_py.Audio]`

### TypeScript

- Non-streaming: `Audio | null`
- Streaming: `Audio | null`

### Go

- Non-streaming: `*Audio`
- Streaming: `*types.Audio`

### Rust

- Non-streaming: `Option<Audio>`
- Streaming: `Option<types::Audio>`

---

# Optional Types

## optional_string

```baml
class T { f string? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[str]`
- Streaming: `typing.Optional[str]`

### TypeScript

- Non-streaming: `string | null`
- Streaming: `string | null`

### Go

- Non-streaming: `*string`
- Streaming: `*string`

### Rust

- Non-streaming: `Option<String>`
- Streaming: `Option<String>`

---

## optional_int

```baml
class T { f int? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

# Literal Types

## literal_string

```baml
class T { f "hello" }
```

### target: `T.f`

### Python

- Non-streaming: `typing_extensions.Literal['hello']`
- Streaming: `typing.Optional[typing_extensions.Literal['hello']]`

### TypeScript

- Non-streaming: `"hello"`
- Streaming: `"hello" | null`

### Go

- Non-streaming: `string`
- Streaming: `*string`

### Rust

- Non-streaming: `String`
- Streaming: `Option<String>`

---

## literal_int

```baml
class T { f 42 }
```

### target: `T.f`

### Python

- Non-streaming: `typing_extensions.Literal[42]`
- Streaming: `typing.Optional[typing_extensions.Literal[42]]`

### TypeScript

- Non-streaming: `42`
- Streaming: `42 | null`

### Go

- Non-streaming: `int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `i64`
- Streaming: `Option<i64>`

---

## literal_bool_true

```baml
class T { f true }
```

### target: `T.f`

### Python

- Non-streaming: `typing_extensions.Literal[True]`
- Streaming: `typing.Optional[typing_extensions.Literal[True]]`

### TypeScript

- Non-streaming: `true`
- Streaming: `true | null`

### Go

- Non-streaming: `bool`
- Streaming: `*bool`

### Rust

- Non-streaming: `bool`
- Streaming: `Option<bool>`

---

## literal_bool_false

```baml
class T { f false }
```

### target: `T.f`

### Python

- Non-streaming: `typing_extensions.Literal[False]`
- Streaming: `typing.Optional[typing_extensions.Literal[False]]`

### TypeScript

- Non-streaming: `false`
- Streaming: `false | null`

### Go

- Non-streaming: `bool`
- Streaming: `*bool`

### Rust

- Non-streaming: `bool`
- Streaming: `Option<bool>`

---

# Collection Types

## list_of_strings

```baml
class T { f string[] }
```

### target: `T.f`

### Python

- Non-streaming: `typing.List[str]`
- Streaming: `typing.List[str]`

### TypeScript

- Non-streaming: `string[]`
- Streaming: `string[]`

### Go

- Non-streaming: `[]string`
- Streaming: `[]string`

### Rust

- Non-streaming: `Vec<String>`
- Streaming: `Vec<String>`

---

## list_of_ints

```baml
class T { f int[] }
```

### target: `T.f`

### Python

- Non-streaming: `typing.List[int]`
- Streaming: `typing.List[int]`

### TypeScript

- Non-streaming: `number[]`
- Streaming: `number[]`

### Go

- Non-streaming: `[]int64`
- Streaming: `[]int64`

### Rust

- Non-streaming: `Vec<i64>`
- Streaming: `Vec<i64>`

---

## nested_list

```baml
class T { f string[][] }
```

### target: `T.f`

### Python

- Non-streaming: `typing.List[typing.List[str]]`
- Streaming: `typing.List[typing.List[str]]`

### TypeScript

- Non-streaming: `string[][]`
- Streaming: `string[][]`

### Go

- Non-streaming: `[][]string`
- Streaming: `[][]string`

### Rust

- Non-streaming: `Vec<Vec<String>>`
- Streaming: `Vec<Vec<String>>`

---

## optional_list

```baml
class T { f string[]? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[typing.List[str]]`
- Streaming: `typing.Optional[typing.List[str]]`

### TypeScript

- Non-streaming: `string[] | null`
- Streaming: `string[] | null`

### Go

- Non-streaming: `*[]string`
- Streaming: `*[]string`

### Rust

- Non-streaming: `Option<Vec<String>>`
- Streaming: `Option<Vec<String>>`

---

## map_string_to_int

```baml
class T { f map<string, int> }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Dict[str, int]`
- Streaming: `typing.Dict[str, int]`

### TypeScript

- Non-streaming: `Record<string, number>`
- Streaming: `Record<string, number>`

### Go

- Non-streaming: `map[string]int64`
- Streaming: `map[string]int64`

### Rust

- Non-streaming: `std::collections::HashMap<String, i64>`
- Streaming: `std::collections::HashMap<String, i64>`

---

## map_string_to_string

```baml
class T { f map<string, string> }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Dict[str, str]`
- Streaming: `typing.Dict[str, str]`

### TypeScript

- Non-streaming: `Record<string, string>`
- Streaming: `Record<string, string>`

### Go

- Non-streaming: `map[string]string`
- Streaming: `map[string]string`

### Rust

- Non-streaming: `std::collections::HashMap<String, String>`
- Streaming: `std::collections::HashMap<String, String>`

---

## optional_map

```baml
class T { f map<string, int>? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[typing.Dict[str, int]]`
- Streaming: `typing.Optional[typing.Dict[str, int]]`

### TypeScript

- Non-streaming: `Record<string, number> | null`
- Streaming: `Record<string, number> | null`

### Go

- Non-streaming: `*map[string]int64`
- Streaming: `*map[string]int64`

### Rust

- Non-streaming: `Option<std::collections::HashMap<String, i64>>`
- Streaming: `Option<std::collections::HashMap<String, i64>>`

---

## map_of_lists

```baml
class T { f map<string, int[]> }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Dict[str, typing.List[int]]`
- Streaming: `typing.Dict[str, typing.List[int]]`

### TypeScript

- Non-streaming: `Record<string, number[]>`
- Streaming: `Record<string, number[]>`

### Go

- Non-streaming: `map[string][]int64`
- Streaming: `map[string][]int64`

### Rust

- Non-streaming: `std::collections::HashMap<String, Vec<i64>>`
- Streaming: `std::collections::HashMap<String, Vec<i64>>`

---

# Union Types

## union_int_string

```baml
class T { f int | string }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_int_string_bool

```baml
class T { f int | string | bool }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str, bool]`
- Streaming: `typing.Optional[typing.Union[int, str, bool]]`

### TypeScript

- Non-streaming: `number | string | boolean`
- Streaming: `number | string | boolean | null`

### Go

- Non-streaming: `Union3BoolOrIntOrString`
- Streaming: `*types.Union3BoolOrIntOrString`

### Rust

- Non-streaming: `Union3BoolOrIntOrString`
- Streaming: `Option<types::Union3BoolOrIntOrString>`

---

## optional_union

```baml
class T { f (int | string)? }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[typing.Union[int, str]]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string | null`
- Streaming: `number | string | null`

### Go

- Non-streaming: `*Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Option<Union2IntOrString>`
- Streaming: `Option<types::Union2IntOrString>`

---

# Class References

## class_reference

```baml
class Inner { x int }
class Outer { inner Inner }
```

### target: `Outer.inner`

### Python

- Non-streaming: `Inner`
- Streaming: `typing.Optional["Inner"]`

### TypeScript

- Non-streaming: `Inner`
- Streaming: `Inner | null`

### Go

- Non-streaming: `Inner`
- Streaming: `*Inner`

### Rust

- Non-streaming: `Inner`
- Streaming: `Option<Inner>`

---

## nested_class_c_to_b

```baml
class A { x int }
class B { a A }
class C { b B }
```

### target: `C.b`

### Python

- Non-streaming: `B`
- Streaming: `typing.Optional["B"]`

### TypeScript

- Non-streaming: `B`
- Streaming: `B | null`

### Go

- Non-streaming: `B`
- Streaming: `*B`

### Rust

- Non-streaming: `B`
- Streaming: `Option<B>`

---

## nested_class_b_to_a

```baml
class A { x int }
class B { a A }
class C { b B }
```

### target: `B.a`

### Python

- Non-streaming: `A`
- Streaming: `typing.Optional["A"]`

### TypeScript

- Non-streaming: `A`
- Streaming: `A | null`

### Go

- Non-streaming: `A`
- Streaming: `*A`

### Rust

- Non-streaming: `A`
- Streaming: `Option<A>`

---

# Enum References

## enum_reference

```baml
enum Status {
    Active
    Inactive
}
class T { status Status }
```

### target: `T.status`

### Python

- Non-streaming: `Status`
- Streaming: `typing.Optional[types.Status]`

### TypeScript

- Non-streaming: `Status`
- Streaming: `types.Status | null`

### Go

- Non-streaming: `Status`
- Streaming: `*types.Status`

### Rust

- Non-streaming: `Status`
- Streaming: `Option<types::Status>`

---

## optional_enum

```baml
enum Status {
    Active
    Inactive
}
class T { status Status? }
```

### target: `T.status`

### Python

- Non-streaming: `typing.Optional[Status]`
- Streaming: `typing.Optional[types.Status]`

### TypeScript

- Non-streaming: `Status | null`
- Streaming: `types.Status | null`

### Go

- Non-streaming: `*Status`
- Streaming: `*types.Status`

### Rust

- Non-streaming: `Option<Status>`
- Streaming: `Option<types::Status>`

---

# Streaming Attributes

## stream_with_state_string

```baml
class T { f string @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `str`
- Streaming: `StreamState[typing.Optional[str]]`

### TypeScript

- Non-streaming: `string`
- Streaming: `StreamState<string | null>`

### Go

- Non-streaming: `string`
- Streaming: `baml.StreamState[*string]`

### Rust

- Non-streaming: `String`
- Streaming: `baml::StreamState<Option<String>>`

---

## stream_with_state_int

```baml
class T { f int @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `int`
- Streaming: `StreamState[typing.Optional[int]]`

### TypeScript

- Non-streaming: `number`
- Streaming: `StreamState<number | null>`

### Go

- Non-streaming: `int64`
- Streaming: `baml.StreamState[*int64]`

### Rust

- Non-streaming: `i64`
- Streaming: `baml::StreamState<Option<i64>>`

---

## stream_with_state_optional

```baml
class T { f string? @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[str]`
- Streaming: `StreamState[typing.Optional[str]]`

### TypeScript

- Non-streaming: `string | null`
- Streaming: `StreamState<string | null>`

### Go

- Non-streaming: `*string`
- Streaming: `baml.StreamState[*string]`

### Rust

- Non-streaming: `Option<String>`
- Streaming: `baml::StreamState<Option<String>>`

---

## stream_not_null_string

```baml
class T { f string @stream.not_null }
```

### target: `T.f`

### Python

- Non-streaming: `str`
- Streaming: `str`

### TypeScript

- Non-streaming: `string`
- Streaming: `string`

### Go

- Non-streaming: `string`
- Streaming: `string`

### Rust

- Non-streaming: `String`
- Streaming: `String`

---

## stream_not_null_int

```baml
class T { f int @stream.not_null }
```

### target: `T.f`

### Python

- Non-streaming: `int`
- Streaming: `int`

### TypeScript

- Non-streaming: `number`
- Streaming: `number`

### Go

- Non-streaming: `int64`
- Streaming: `int64`

### Rust

- Non-streaming: `i64`
- Streaming: `i64`

---

## stream_state_inside_union

```baml
class T { f (int @stream.with_state | string) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[StreamState[int], str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number> | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*Union2StreamStateIntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<Union2StreamStateIntOrString>`

---

## stream_not_null_with_state

```baml
class T { f string @stream.not_null @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `str`
- Streaming: `StreamState[str]`

### TypeScript

- Non-streaming: `string`
- Streaming: `StreamState<string>`

### Go

- Non-streaming: `string`
- Streaming: `baml.StreamState[string]`

### Rust

- Non-streaming: `String`
- Streaming: `baml::StreamState<String>`

---

## stream_done

```baml
class Inner { x int }
class T { inner Inner @stream.done }
```

### target: `T.inner`

### Python

- Non-streaming: `Inner`
- Streaming: `typing.Optional["types.Inner"]`

### TypeScript

- Non-streaming: `Inner`
- Streaming: `types.Inner | null`

### Go

- Non-streaming: `Inner`
- Streaming: `*types.Inner`

### Rust

- Non-streaming: `Inner`
- Streaming: `Option<types::Inner>`

---

## stream_done_with_state

```baml
class Inner { x int }
class T { inner Inner @stream.done @stream.with_state }
```

### target: `T.inner`

### Python

- Non-streaming: `Inner`
- Streaming: `StreamState[typing.Optional["types.Inner"]]`

### TypeScript

- Non-streaming: `Inner`
- Streaming: `StreamState<types.Inner | null>`

### Go

- Non-streaming: `Inner`
- Streaming: `baml.StreamState[*types.Inner]`

### Rust

- Non-streaming: `Inner`
- Streaming: `baml::StreamState<Option<types::Inner>>`

---

## list_of_stream_done_classes

```baml
class Inner { x int }
class T { items (Inner @stream.done)[] }
```

### target: `T.items`

### Python

- Non-streaming: `typing.List["Inner"]`
- Streaming: `typing.List["types.Inner"]`

### TypeScript

- Non-streaming: `Inner[]`
- Streaming: `types.Inner[]`

### Go

- Non-streaming: `[]Inner`
- Streaming: `[]types.Inner`

### Rust

- Non-streaming: `Vec<Inner>`
- Streaming: `Vec<types::Inner>`

---

## list_field_with_stream_done

```baml
class Inner { x int }
class T { items Inner[] @stream.done }
```

### target: `T.items`

### Python

- Non-streaming: `typing.List["Inner"]`
- Streaming: `typing.List["types.Inner"]`

### TypeScript

- Non-streaming: `Inner[]`
- Streaming: `types.Inner[]`

### Go

- Non-streaming: `[]Inner`
- Streaming: `[]types.Inner`

### Rust

- Non-streaming: `Vec<Inner>`
- Streaming: `Vec<types::Inner>`

---

## nested_list_with_stream_done

```baml
class Inner { x int }
class T { matrix Inner[][][] @stream.done }
```

### target: `T.matrix`

### Python

- Non-streaming: `typing.List[typing.List[typing.List["Inner"]]]`
- Streaming: `typing.List[typing.List[typing.List["types.Inner"]]]`

### TypeScript

- Non-streaming: `Inner[][][]`
- Streaming: `types.Inner[][][]`

### Go

- Non-streaming: `[][][]Inner`
- Streaming: `[][][]types.Inner`

### Rust

- Non-streaming: `Vec<Vec<Vec<Inner>>>`
- Streaming: `Vec<Vec<Vec<types::Inner>>>`

---

## map_with_stream_done

```baml
class Inner { x int }
class T { lookup map<string, Inner> @stream.done }
```

### target: `T.lookup`

### Python

- Non-streaming: `typing.Dict[str, "Inner"]`
- Streaming: `typing.Dict[str, "types.Inner"]`

### TypeScript

- Non-streaming: `Record<string, Inner>`
- Streaming: `Record<string, types.Inner>`

### Go

- Non-streaming: `map[string]Inner`
- Streaming: `map[string]types.Inner`

### Rust

- Non-streaming: `std::collections::HashMap<String, Inner>`
- Streaming: `std::collections::HashMap<String, types::Inner>`

---

# Union Types with Streaming Attributes

## union_with_stream_done_variant

```baml
class A { x int }
class T { f int @stream.done | string }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_with_class_variants

```baml
class A { x int }
class B { y string }
class T { f A | B }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union["A", "B"]`
- Streaming: `typing.Optional[typing.Union["A", "B"]]`

### TypeScript

- Non-streaming: `A | B`
- Streaming: `A | B | null`

### Go

- Non-streaming: `Union2AOrB`
- Streaming: `*Union2AOrB`

### Rust

- Non-streaming: `Union2AOrB`
- Streaming: `Option<Union2AOrB>`

---

## union_class_with_primitive

```baml
class Inner { x int }
class T { f Inner | string }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union["Inner", str]`
- Streaming: `typing.Optional[typing.Union["Inner", str]]`

### TypeScript

- Non-streaming: `Inner | string`
- Streaming: `Inner | string | null`

### Go

- Non-streaming: `Union2InnerOrString`
- Streaming: `*Union2InnerOrString`

### Rust

- Non-streaming: `Union2InnerOrString`
- Streaming: `Option<Union2InnerOrString>`

---

## union_with_stream_not_null

```baml
class T { f (int | string) @stream.not_null }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Union[int, str]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `types::Union2IntOrString`

---

## union_with_stream_with_state

```baml
class T { f (int | string) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `StreamState[typing.Optional[typing.Union[int, str]]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number | string | null>`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `baml.StreamState[*types.Union2IntOrString]`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `baml::StreamState<Option<types::Union2IntOrString>>`

---

# Check Attributes

## check_on_primitive

```baml
class T { age int @check(valid_age, {{ this >= 0 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['valid_age']]`
- Streaming:

```python
typing.Optional[types.Checked[int, typing_extensions.Literal['valid_age']]]
```

### TypeScript

- Non-streaming: `Checked<number,"valid_age">`
- Streaming: `types.Checked<number,"valid_age"> | null`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `*types.Checked[int64]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `Option<types::Checked<i64>>`

---

## check_on_optional

```baml
class T { age int? @check(valid_age, {{ this >= 0 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `Checked[typing.Optional[int], typing_extensions.Literal['valid_age']]`
- Streaming: `types.Checked[typing.Optional[int], typing_extensions.Literal['valid_age']]`

### TypeScript

- Non-streaming: `Checked<number | null,"valid_age">`
- Streaming: `types.Checked<number | null,"valid_age">`

### Go

- Non-streaming: `Checked[*int64]`
- Streaming: `types.Checked[*int64]`

### Rust

- Non-streaming: `Checked<Option<i64>>`
- Streaming: `types::Checked<Option<i64>>`

---

## check_on_optional_with_outer_null

```baml
class T { f (int? @check(valid, {{ this >= 0 }})) | null }
```

### target: `T.f`

### Python

- Non-streaming:

```python
Checked[typing.Optional[int], typing_extensions.Literal['valid']]
```

- Streaming:

```python
types.Checked[typing.Optional[int], typing_extensions.Literal['valid']]
```

### TypeScript

- Non-streaming: `Checked<number | null,"valid">`
- Streaming: `types.Checked<number | null,"valid">`

### Go

- Non-streaming: `Checked[*int64]`
- Streaming: `types.Checked[*int64]`

### Rust

- Non-streaming: `Checked<Option<i64>>`
- Streaming: `types::Checked<Option<i64>>`

---

## check_with_stream_not_null

```baml
class T { age int @check(valid_age, {{ this >= 0 }}) @stream.not_null }
```

### target: `T.age`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['valid_age']]`
- Streaming: `types.Checked[int, typing_extensions.Literal['valid_age']]`

### TypeScript

- Non-streaming: `Checked<number,"valid_age">`
- Streaming: `types.Checked<number,"valid_age">`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `types.Checked[int64]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `types::Checked<i64>`

---

## check_with_stream_with_state

```baml
class T { age int @check(valid_age, {{ this >= 0 }}) @stream.with_state }
```

### target: `T.age`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['valid_age']]`
- Streaming:

```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['valid_age']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"valid_age">`
- Streaming: `StreamState<types.Checked<number,"valid_age"> | null>`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `baml.StreamState[*types.Checked[int64]]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `baml::StreamState<Option<types::Checked<i64>>>`

---

## multiple_checks

```baml
class T { age int @check(positive, {{ this > 0 }}) @check(small, {{ this < 100 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['positive', 'small']]`
- Streaming:

```python
typing.Optional[types.Checked[int, typing_extensions.Literal['positive', 'small']]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive" | "small">`
- Streaming: `types.Checked<number,"positive" | "small"> | null`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `*types.Checked[int64]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `Option<types::Checked<i64>>`

---

# Assert Attributes

## assert_on_primitive

```baml
class T { age int @assert(valid_age, {{ this >= 0 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `int`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number`
- Streaming: `number | null`

### Go

- Non-streaming: `int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `i64`
- Streaming: `Option<i64>`

---

## assert_on_optional

```baml
class T { age int? @assert(valid_age, {{ this >= 0 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

## assert_on_optional_with_outer_null

```baml
class T { f (int? @assert(valid, {{ this >= 0 }})) | null }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

## assert_with_stream_not_null

```baml
class T { age int @assert(valid_age, {{ this >= 0 }}) @stream.not_null }
```

### target: `T.age`

### Python

- Non-streaming: `int`
- Streaming: `int`

### TypeScript

- Non-streaming: `number`
- Streaming: `number`

### Go

- Non-streaming: `int64`
- Streaming: `int64`

### Rust

- Non-streaming: `i64`
- Streaming: `i64`

---

## assert_with_stream_with_state

```baml
class T { age int @assert(valid_age, {{ this >= 0 }}) @stream.with_state }
```

### target: `T.age`

### Python

- Non-streaming: `int`
- Streaming: `StreamState[typing.Optional[int]]`

### TypeScript

- Non-streaming: `number`
- Streaming: `StreamState<number | null>`

### Go

- Non-streaming: `int64`
- Streaming: `baml.StreamState[*int64]`

### Rust

- Non-streaming: `i64`
- Streaming: `baml::StreamState<Option<i64>>`

---

## multiple_asserts

```baml
class T { age int @assert(positive, {{ this > 0 }}) @assert(small, {{ this < 100 }}) }
```

### target: `T.age`

### Python

- Non-streaming: `int`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number`
- Streaming: `number | null`

### Go

- Non-streaming: `int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `i64`
- Streaming: `Option<i64>`

---

# Complex Union Compositions

## union_variant_stream_done_union_stream_with_state

```baml
class T { f (int @stream.done | string) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `StreamState[typing.Optional[typing.Union[int, str]]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number | string | null>`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `baml.StreamState[*types.Union2IntOrString]`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `baml::StreamState<Option<types::Union2IntOrString>>`

---

## union_variant_stream_not_null_union_stream_with_state

```baml
class T { f (int @stream.not_null | string) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `StreamState[typing.Optional[typing.Union[int, str]]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number | string | null>`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `baml.StreamState[*types.Union2IntOrString]`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `baml::StreamState<Option<types::Union2IntOrString>>`

---

## union_different_variant_attributes

```baml
class T { f int @stream.done | string @stream.not_null }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Union[int, str]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `types::Union2IntOrString`

---

## union_with_check_on_variant

```baml
class T { f int @check(positive, {{ this > 0 }}) | string }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['positive']], str]
```

- Streaming:

```python
typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], str]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive"> | string`
- Streaming: `types.Checked<number,"positive"> | string | null`

### Go

- Non-streaming: `Union2CheckedIntOrString`
- Streaming: `*types.Union2CheckedIntOrString`

### Rust

- Non-streaming: `Union2CheckedIntOrString`
- Streaming: `Option<types::Union2CheckedIntOrString>`

---

## union_with_check_on_whole_union

```baml
class T { f (int | string) @check(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[typing.Union[int, str], typing_extensions.Literal['valid']]`
- Streaming:

```python
typing.Optional[types.Checked[typing.Union[int, str], typing_extensions.Literal['valid']]]
```

### TypeScript

- Non-streaming: `Checked<number | string,"valid">`
- Streaming: `types.Checked<number | string,"valid"> | null`

### Go

- Non-streaming: `Checked[Union2IntOrString]`
- Streaming: `*types.Checked[types.Union2IntOrString]`

### Rust

- Non-streaming: `Checked<Union2IntOrString>`
- Streaming: `Option<types::Checked<types::Union2IntOrString>>`

---

## union_with_check_on_whole_union_optional

```baml
class T { f (int | string | null) @check(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
Checked[typing.Optional[typing.Union[int, str]], typing_extensions.Literal['valid']]
```

- Streaming:

```python
types.Checked[typing.Optional[typing.Union[int, str]], typing_extensions.Literal['valid']]
```

### TypeScript

- Non-streaming: `Checked<number | string | null,"valid">`
- Streaming: `types.Checked<number | string | null,"valid">`

### Go

- Non-streaming: `*Checked[Union2IntOrString]`
- Streaming: `*types.Checked[types.Union2IntOrString]`

### Rust

- Non-streaming: `Option<Checked<Union2IntOrString>>`
- Streaming: `Option<types::Checked<types::Union2IntOrString>>`

---

## union_check_on_variant_and_whole

```baml
class T { f (int @check(positive, {{ this > 0 }}) | string) @check(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
Checked[typing.Union[Checked[int, typing_extensions.Literal['positive']], str], typing_extensions.Literal['valid']]
```

- Streaming:

```python
typing.Optional[types.Checked[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], str], typing_extensions.Literal['valid']]]
```

### TypeScript

- Non-streaming: `Checked<Checked<number,"positive"> | string,"valid">`
- Streaming: `types.Checked<types.Checked<number,"positive"> | string,"valid"> | null`

### Go

- Non-streaming: `Checked[Union2CheckedIntOrString]`
- Streaming: `*types.Checked[types.Union2CheckedIntOrString]`

### Rust

- Non-streaming: `Checked<Union2CheckedIntOrString>`
- Streaming: `Option<types::Checked<types::Union2CheckedIntOrString>>`

---

## union_check_and_stream_attrs_mixed

```baml
class T { f (int @check(positive, {{ this > 0 }}) | string) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['positive']], str]
```

- Streaming:

```python
StreamState[typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], str]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive"> | string`
- Streaming: `StreamState<types.Checked<number,"positive"> | string | null>`

### Go

- Non-streaming: `Union2CheckedIntOrString`
- Streaming: `baml.StreamState[*types.Union2CheckedIntOrString]`

### Rust

- Non-streaming: `Union2CheckedIntOrString`
- Streaming: `baml::StreamState<Option<types::Union2CheckedIntOrString>>`

---

## union_all_attrs_combined

```baml
class T { f (int @check(positive, {{ this > 0 }}) @stream.done | string) @check(valid, {{ true }}) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming:

```python
Checked[typing.Union[Checked[int, typing_extensions.Literal['positive']], str], typing_extensions.Literal['valid']]
```

- Streaming:

```python
StreamState[typing.Optional[types.Checked[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], str], typing_extensions.Literal['valid']]]]
```

### TypeScript

- Non-streaming: `Checked<Checked<number,"positive"> | string,"valid">`
- Streaming:

```typescript
StreamState<types.Checked<types.Checked<number,"positive"> | string,"valid"> | null>
```

### Go

- Non-streaming: `Checked[Union2CheckedIntOrString]`
- Streaming: `baml.StreamState[*types.Checked[types.Union2CheckedIntOrString]]`

### Rust

- Non-streaming: `Checked<Union2CheckedIntOrString>`
- Streaming: `baml::StreamState<Option<types::Checked<types::Union2CheckedIntOrString>>>`

---

## union_with_assert_on_variant

```baml
class T { f int @assert(positive, {{ this > 0 }}) | string }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_with_assert_on_whole_union

```baml
class T { f (int | string) @assert(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_with_assert_on_whole_union_optional

```baml
class T { f (int | string | null) @assert(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[typing.Union[int, str]]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string | null`
- Streaming: `number | string | null`

### Go

- Non-streaming: `*Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Option<Union2IntOrString>`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_assert_on_variant_and_whole

```baml
class T { f (int @assert(positive, {{ this > 0 }}) | string) @assert(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## union_assert_and_stream_attrs_mixed

```baml
class T { f (int @assert(positive, {{ this > 0 }}) | string) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `StreamState[typing.Optional[typing.Union[int, str]]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number | string | null>`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `baml.StreamState[*types.Union2IntOrString]`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `baml::StreamState<Option<types::Union2IntOrString>>`

---

## union_all_attrs_combined_with_assert

```baml
class T { f (int @assert(positive, {{ this > 0 }}) @stream.done | string) @assert(valid, {{ true }}) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `StreamState[typing.Optional[typing.Union[int, str]]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `StreamState<number | string | null>`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `baml.StreamState[*types.Union2IntOrString]`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `baml::StreamState<Option<types::Union2IntOrString>>`

---

# Check Simplification Scenarios

## check_simplification_scenario_1_same_check_all_variants

```baml
class T { f (int @check(valid, {{ this > 0 }})) | (string @check(valid, {{ this > 0 }})) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['valid']], Checked[str, typing_extensions.Literal['valid']]]
```

- Streaming:

```python
typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['valid']], types.Checked[str, typing_extensions.Literal['valid']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"valid"> | Checked<string,"valid">`
- Streaming: `types.Checked<number,"valid"> | types.Checked<string,"valid"> | null`

### Go

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `*types.Union2CheckedIntOrCheckedString`

### Rust

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `Option<types::Union2CheckedIntOrCheckedString>`

---

## check_simplification_scenario_2_same_name_diff_expr

```baml
class T { f (int @check(valid, {{ this > 0 }})) | (string @check(valid, {{ this != "" }})) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['valid']], Checked[str, typing_extensions.Literal['valid']]]
```

- Streaming:

```python
typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['valid']], types.Checked[str, typing_extensions.Literal['valid']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"valid"> | Checked<string,"valid">`
- Streaming: `types.Checked<number,"valid"> | types.Checked<string,"valid"> | null`

### Go

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `*types.Union2CheckedIntOrCheckedString`

### Rust

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `Option<types::Union2CheckedIntOrCheckedString>`

---

## check_simplification_scenario_3a_diff_names

```baml
class T { f (int @check(positive, {{ this > 0 }})) | (string @check(non_empty, {{ this != "" }})) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['positive']], Checked[str, typing_extensions.Literal['non_empty']]]
```

- Streaming:

```python
typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], types.Checked[str, typing_extensions.Literal['non_empty']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive"> | Checked<string,"non_empty">`
- Streaming: `types.Checked<number,"positive"> | types.Checked<string,"non_empty"> | null`

### Go

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `*types.Union2CheckedIntOrCheckedString`

### Rust

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `Option<types::Union2CheckedIntOrCheckedString>`

---

## check_simplification_scenario_3b_diff_names_same_expr

```baml
class T { f (int @check(positive, {{ true }})) | (string @check(non_empty, {{ true }})) }
```

### target: `T.f`

### Python

- Non-streaming:

```python
typing.Union[Checked[int, typing_extensions.Literal['positive']], Checked[str, typing_extensions.Literal['non_empty']]]
```

- Streaming:

```python
typing.Optional[typing.Union[types.Checked[int, typing_extensions.Literal['positive']], types.Checked[str, typing_extensions.Literal['non_empty']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive"> | Checked<string,"non_empty">`
- Streaming: `types.Checked<number,"positive"> | types.Checked<string,"non_empty"> | null`

### Go

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `*types.Union2CheckedIntOrCheckedString`

### Rust

- Non-streaming: `Union2CheckedIntOrCheckedString`
- Streaming: `Option<types::Union2CheckedIntOrCheckedString>`

---

## check_simplification_scenario_4_checked_union_with_unchecked

```baml
class T { f (int | string) @check(valid, {{ true }}) | string }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[typing.Union[int, str], typing_extensions.Literal['valid']]`
- Streaming: `typing.Optional[types.Checked[typing.Union[int, str], typing_extensions.Literal['valid']]]`

### TypeScript

- Non-streaming: `Checked<number | string,"valid">`
- Streaming: `types.Checked<number | string,"valid"> | null`

### Go

- Non-streaming: `Checked[Union2IntOrString]`
- Streaming: `*types.Checked[types.Union2IntOrString]`

### Rust

- Non-streaming: `Checked<Union2IntOrString>`
- Streaming: `Option<types::Checked<types::Union2IntOrString>>`

---

## check_simplification_scenario_5_checked_union_with_unchecked_reverse

```baml
class T { f string | (int | string) @check(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[typing.Union[str, int], typing_extensions.Literal['valid']]`
- Streaming:

```python
typing.Optional[types.Checked[typing.Union[str, int], typing_extensions.Literal['valid']]]
```

### TypeScript

- Non-streaming: `Checked<string | number,"valid">`
- Streaming: `types.Checked<string | number,"valid"> | null`

### Go

- Non-streaming: `Checked[Union2IntOrString]`
- Streaming: `*types.Checked[types.Union2IntOrString]`

### Rust

- Non-streaming: `Checked<Union2IntOrString>`
- Streaming: `Option<types::Checked<types::Union2IntOrString>>`

---

## check_simplification_scenario_7_checked_union_with_unchecked_null

```baml
class T { f (int | null) @check(valid, {{ true }}) | null }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[typing.Optional[int], typing_extensions.Literal['valid']]`
- Streaming:

```python
types.Checked[typing.Optional[int], typing_extensions.Literal['valid']]
```

### TypeScript

- Non-streaming: `Checked<number | null,"valid">`
- Streaming: `types.Checked<number | null,"valid">`

### Go

- Non-streaming: `Checked[*int64]`
- Streaming: `types.Checked[*int64]`

### Rust

- Non-streaming: `Checked<Option<i64>>`
- Streaming: `types::Checked<Option<i64>>`

---

## check_simplification_scenario_8_checked_union_with_unchecked_null_reverse

```baml
class T { f null | (int | null) @check(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[typing.Optional[int], typing_extensions.Literal['valid']]`
- Streaming:

```python
types.Checked[typing.Optional[int], typing_extensions.Literal['valid']]
```

### TypeScript

- Non-streaming: `Checked<number | null,"valid">`
- Streaming: `types.Checked<number | null,"valid">`

### Go

- Non-streaming: `Checked[*int64]`
- Streaming: `types.Checked[*int64]`

### Rust

- Non-streaming: `Checked<Option<i64>>`
- Streaming: `types::Checked<Option<i64>>`

---

# Assert Simplification Scenarios

## assert_simplification_scenario_1_same_assert_all_variants

```baml
class T { f (int @assert(valid, {{ this > 0 }})) | (string @assert(valid, {{ this > 0 }})) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_2_same_name_diff_expr

```baml
class T { f (int @assert(valid, {{ this > 0 }})) | (string @assert(valid, {{ this != "" }})) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_3a_diff_names

```baml
class T { f (int @assert(positive, {{ this > 0 }})) | (string @assert(non_empty, {{ this != "" }})) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_3b_diff_names_same_expr

```baml
class T { f (int @assert(positive, {{ true }})) | (string @assert(non_empty, {{ true }})) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_4_asserted_union_with_unasserted

```baml
class T { f (int | string) @assert(valid, {{ true }}) | string }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[int, str]`
- Streaming: `typing.Optional[typing.Union[int, str]]`

### TypeScript

- Non-streaming: `number | string`
- Streaming: `number | string | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_5_asserted_union_with_unasserted_reverse

```baml
class T { f string | (int | string) @assert(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Union[str, int]`
- Streaming: `typing.Optional[typing.Union[str, int]]`

### TypeScript

- Non-streaming: `string | number`
- Streaming: `string | number | null`

### Go

- Non-streaming: `Union2IntOrString`
- Streaming: `*types.Union2IntOrString`

### Rust

- Non-streaming: `Union2IntOrString`
- Streaming: `Option<types::Union2IntOrString>`

---

## assert_simplification_scenario_7_asserted_union_with_unasserted_null

```baml
class T { f (int | null) @assert(valid, {{ true }}) | null }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

## assert_simplification_scenario_8_asserted_union_with_unasserted_null_reverse

```baml
class T { f null | (int | null) @assert(valid, {{ true }}) }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

# Type Aliases

## type_alias_string_list

```baml
type StringList = string[]
```

### target: `StringList`

### Python

- Non-streaming: `typing.List[str]`
- Streaming: `typing.List[str]`

### TypeScript

- Non-streaming: `string[]`
- Streaming: `string[]`

### Go

- Non-streaming: `[]string`
- Streaming: `[]string`

### Rust

- Non-streaming: `Vec<String>`
- Streaming: `Vec<String>`

---

## type_alias_int_map

```baml
type IntMap = map<string, int>
```

### target: `IntMap`

### Python

- Non-streaming: `typing.Dict[str, int]`
- Streaming: `typing.Dict[str, int]`

### TypeScript

- Non-streaming: `Record<string, number>`
- Streaming: `Record<string, number>`

### Go

- Non-streaming: `map[string]int64`
- Streaming: `map[string]int64`

### Rust

- Non-streaming: `std::collections::HashMap<String, i64>`
- Streaming: `std::collections::HashMap<String, i64>`

---

## type_alias_maybe_int

```baml
type MaybeInt = int?
```

### target: `MaybeInt`

### Python

- Non-streaming: `typing.Optional[int]`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number | null`
- Streaming: `number | null`

### Go

- Non-streaming: `*int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `Option<i64>`
- Streaming: `Option<i64>`

---

# Complex Nested Types

## list_of_maps

```baml
class T { f map<string, int>[] }
```

### target: `T.f`

### Python

- Non-streaming: `typing.List[typing.Dict[str, int]]`
- Streaming: `typing.List[typing.Dict[str, int]]`

### TypeScript

- Non-streaming: `Record<string, number>[]`
- Streaming: `Record<string, number>[]`

### Go

- Non-streaming: `[]map[string]int64`
- Streaming: `[]map[string]int64`

### Rust

- Non-streaming: `Vec<std::collections::HashMap<String, i64>>`
- Streaming: `Vec<std::collections::HashMap<String, i64>>`

---

## map_of_string_lists

```baml
class T { f map<string, string[]> }
```

### target: `T.f`

### Python

- Non-streaming: `typing.Dict[str, typing.List[str]]`
- Streaming: `typing.Dict[str, typing.List[str]]`

### TypeScript

- Non-streaming: `Record<string, string[]>`
- Streaming: `Record<string, string[]>`

### Go

- Non-streaming: `map[string][]string`
- Streaming: `map[string][]string`

### Rust

- Non-streaming: `std::collections::HashMap<String, Vec<String>>`
- Streaming: `std::collections::HashMap<String, Vec<String>>`

---

## list_of_optionals

```baml
class T { f (string?)[] }
```

### target: `T.f`

### Python

- Non-streaming: `typing.List[typing.Optional[str]]`
- Streaming: `typing.List[typing.Optional[str]]`

### TypeScript

- Non-streaming: `(string | null)[]`
- Streaming: `(string | null)[]`

### Go

- Non-streaming: `[]*string`
- Streaming: `[]*string`

### Rust

- Non-streaming: `Vec<Option<String>>`
- Streaming: `Vec<Option<String>>`

---

## stream_state_checked

```baml
class T { f int @stream.with_state @check(positive, {{ this > 0 }}) }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['positive']]`
- Streaming:

```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['positive']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive">`
- Streaming: `StreamState<types.Checked<number,"positive"> | null>`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `baml.StreamState[*types.Checked[int64]]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `baml::StreamState<Option<types::Checked<i64>>>`

---

## checked_stream_state

```baml
class T { f int @check(positive, {{ this > 0 }}) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['positive']]`
- Streaming:

```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['positive']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive">`
- Streaming: `StreamState<types.Checked<number,"positive"> | null>`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `baml.StreamState[*types.Checked[int64]]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `baml::StreamState<Option<types::Checked<i64>>>`

---

## stream_state_checked_paren

```baml
class T { f (int @stream.with_state) @check(positive, {{ this > 0 }}) }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['positive']]`
- Streaming:

```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['positive']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive">`
- Streaming: `StreamState<types.Checked<number,"positive"> | null>`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `baml.StreamState[*types.Checked[int64]]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `baml::StreamState<Option<types::Checked<i64>>>`

---

## checked_stream_state_paren

```baml
class T { f (int @check(positive, {{ this > 0 }})) @stream.with_state }
```

### target: `T.f`

### Python

- Non-streaming: `Checked[int, typing_extensions.Literal['positive']]`
- Streaming:

```python
StreamState[typing.Optional[types.Checked[int, typing_extensions.Literal['positive']]]]
```

### TypeScript

- Non-streaming: `Checked<number,"positive">`
- Streaming: `StreamState<types.Checked<number,"positive"> | null>`

### Go

- Non-streaming: `Checked[int64]`
- Streaming: `baml.StreamState[*types.Checked[int64]]`

### Rust

- Non-streaming: `Checked<i64>`
- Streaming: `baml::StreamState<Option<types::Checked<i64>>>`

---

# Real-World Example

## realistic_task_id

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.id`

### Python

- Non-streaming: `int`
- Streaming: `typing.Optional[int]`

### TypeScript

- Non-streaming: `number`
- Streaming: `number | null`

### Go

- Non-streaming: `int64`
- Streaming: `*int64`

### Rust

- Non-streaming: `i64`
- Streaming: `Option<i64>`

---

## realistic_task_title

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.title`

### Python

- Non-streaming: `str`
- Streaming: `StreamState[typing.Optional[str]]`

### TypeScript

- Non-streaming: `string`
- Streaming: `StreamState<string | null>`

### Go

- Non-streaming: `string`
- Streaming: `baml.StreamState[*string]`

### Rust

- Non-streaming: `String`
- Streaming: `baml::StreamState<Option<String>>`

---

## realistic_task_description

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.description`

### Python

- Non-streaming: `typing.Optional[str]`
- Streaming: `typing.Optional[str]`

### TypeScript

- Non-streaming: `string | null`
- Streaming: `string | null`

### Go

- Non-streaming: `*string`
- Streaming: `*string`

### Rust

- Non-streaming: `Option<String>`
- Streaming: `Option<String>`

---

## realistic_task_priority

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.priority`

### Python

- Non-streaming: `Priority`
- Streaming: `typing.Optional[types.Priority]`

### TypeScript

- Non-streaming: `Priority`
- Streaming: `types.Priority | null`

### Go

- Non-streaming: `Priority`
- Streaming: `*types.Priority`

### Rust

- Non-streaming: `Priority`
- Streaming: `Option<types::Priority>`

---

## realistic_task_tags

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.tags`

### Python

- Non-streaming: `typing.List[str]`
- Streaming: `typing.List[str]`

### TypeScript

- Non-streaming: `string[]`
- Streaming: `string[]`

### Go

- Non-streaming: `[]string`
- Streaming: `[]string`

### Rust

- Non-streaming: `Vec<String>`
- Streaming: `Vec<String>`

---

## realistic_task_metadata

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.metadata`

### Python

- Non-streaming: `typing.Optional[typing.Dict[str, str]]`
- Streaming: `typing.Optional[typing.Dict[str, str]]`

### TypeScript

- Non-streaming: `Record<string, string> | null`
- Streaming: `Record<string, string> | null`

### Go

- Non-streaming: `*map[string]string`
- Streaming: `*map[string]string`

### Rust

- Non-streaming: `Option<std::collections::HashMap<String, String>>`
- Streaming: `Option<std::collections::HashMap<String, String>>`

---

## realistic_task_completed

```baml
enum Priority {
    Low
    Medium
    High
}
class Task {
    id int
    title string @stream.with_state
    description string?
    priority Priority
    tags string[]
    metadata map<string, string>?
    completed bool @stream.not_null
}
```

### target: `Task.completed`

### Python

- Non-streaming: `bool`
- Streaming: `bool`

### TypeScript

- Non-streaming: `boolean`
- Streaming: `boolean`

### Go

- Non-streaming: `bool`
- Streaming: `bool`

### Rust

- Non-streaming: `bool`
- Streaming: `bool`

---

# Enum Value Tests

## enum_color

```baml
enum Color {
    Red
    Green
    Blue
}
```

### target: `Color`

### enum_values: `Red`, `Green`, `Blue`

---

## enum_status

```baml
enum Status {
    Active
    Inactive
    Pending
    Archived
}
```

### target: `Status`

### enum_values: `Active`, `Inactive`, `Pending`, `Archived`

# Block Level Attributes

## block_stream_done

```baml
class Inner {
    x int
    @@stream.done
}
class T { f Inner }
```

### target: `T.f`

### Python

- Non-streaming: `Inner`
- Streaming: `typing.Optional["types.Inner"]`

### TypeScript

- Non-streaming: `Inner`
- Streaming: `types.Inner | null`

### Go

- Non-streaming: `Inner`
- Streaming: `*types.Inner`

### Rust

- Non-streaming: `Inner`
- Streaming: `Option<types::Inner>`

---

## block_stream_done_in_list

```baml
class Inner {
    x int
    @@stream.done
}
class T { list Inner[] }
```

### target: `T.list`

### Python

- Non-streaming: `typing.List["Inner"]`
- Streaming: `typing.List["types.Inner"]`

### TypeScript

- Non-streaming: `Inner[]`
- Streaming: `types.Inner[]`

### Go

- Non-streaming: `[]Inner`
- Streaming: `[]types.Inner`

### Rust

- Non-streaming: `Vec<Inner>`
- Streaming: `Vec<types::Inner>`

---

## nested_block_stream_done_outer

```baml
class Inner {
    x int
    @@stream.done
}
class Middle {
    i Inner
}
class T {
    m Middle
}
```

### target: `T.m`

### Python

- Non-streaming: `Middle`
- Streaming: `typing.Optional["Middle"]`

### TypeScript

- Non-streaming: `Middle`
- Streaming: `Middle | null`

### Go

- Non-streaming: `Middle`
- Streaming: `*Middle`

### Rust

- Non-streaming: `Middle`
- Streaming: `Option<Middle>`

---

## nested_block_stream_done_inner

```baml
class Inner {
    x int
    @@stream.done
}
class Middle {
    i Inner
}
class T {
    m Middle
}
```

### target: `Middle.i`

### Python

- Non-streaming: `Inner`
- Streaming: `typing.Optional["types.Inner"]`

### TypeScript

- Non-streaming: `Inner`
- Streaming: `types.Inner | null`

### Go

- Non-streaming: `Inner`
- Streaming: `*types.Inner`

### Rust

- Non-streaming: `Inner`
- Streaming: `Option<types::Inner>`

---

## block_stream_done_field_access

```baml
class Inner {
    x int
    @@stream.done
}
class T {
    i Inner
}
```

### target: `Inner.x`

### Python

- Non-streaming: `int`
- Streaming: `int`

### TypeScript

- Non-streaming: `number`
- Streaming: `number`

### Go

- Non-streaming: `int64`
- Streaming: `int64`

### Rust

- Non-streaming: `i64`
- Streaming: `i64`

---
