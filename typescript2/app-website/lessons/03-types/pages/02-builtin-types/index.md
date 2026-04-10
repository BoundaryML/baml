# Builtin Types

Types are categories of values. BAML comes with a set of builtin
types similar to those in Python or TypeScript.

## Basic types

 - **int**
 - **float**
 - **bool**
 - **string**
 - **string literal** (e.g. `"up"`, `"admin"`, `"search"`)
 - **image**
 - **audio**

String literals are types that look like string values. They
are useful for talking about very specific strings, or for
collecting a small number of strings into a union.

`image` and `audio` types are for image and audio data.

## Compound types

 - **list**  (e.g. `int[]`, `string[]`)
 - **map** (e.g. `map<string, bool>`)
 - **optional** (e.g. `float?`)
 - **union** (e.g. `int[] | float`)

**Note:** Some types are only allowed as function arguments, not
return types. For example, `image` and `audio`. This is because
we currently only support text-generation models.