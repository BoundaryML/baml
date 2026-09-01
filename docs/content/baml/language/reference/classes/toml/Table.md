---
title: "toml.Table"
description: "Class toml.Table from the generated baml package reference."
---

A TOML [table](https://toml.io/en/latest#table).
Represents a collection of key/value pairs.

```baml
class toml.Table
```

## Fields

### items

```baml
items: map<string, baml.toml.Item>
```

No description is available yet.

## Methods

### parse

```baml
function parse(s: string) -> baml.toml.Table throws baml.toml.TomlParseError
```

No description is available yet.

_Source: `<builtin>/baml/ns_toml/toml.baml:536`_
