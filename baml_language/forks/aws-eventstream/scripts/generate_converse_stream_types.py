#!/usr/bin/env python3
"""Generate `src/converse_stream/types.rs` from the `aws-sdk-bedrockruntime`
source: every type reachable from `ConverseStreamOutput`, plus the exceptions
ConverseStream can send mid-stream.

    python3 scripts/generate_converse_stream_types.py \\
        ~/.cargo/registry/src/*/aws-sdk-bedrockruntime-<version>

Field and variant names come from `src/types/`, JSON keys from the SDK's own
deserializers in `src/protocol_serde/`, so the wire names are exactly the
SDK's. Strings enums and unions become `string_enum!` / `union!` invocations
(macros in `src/converse_stream/mod.rs`) that keep unknown values instead of
failing, as the SDK does.
"""

import html
import os
import re
import subprocess
import sys

ROOT = "ConverseStreamOutput"
# Hand-written in mod.rs: keyed by the `:event-type` header, not JSON.
HANDWRITTEN = {ROOT}
EXCEPTIONS = [
    "InternalServerException",
    "ModelStreamErrorException",
    "ValidationException",
    "ThrottlingException",
    "ServiceUnavailableException",
]
OUT = os.path.join(os.path.dirname(__file__), "..", "src", "converse_stream", "types.rs")


def die(message):
    sys.exit(f"generate_converse_stream_types: {message}")


def main():
    if len(sys.argv) != 2:
        die("usage: generate_converse_stream_types.py <aws-sdk-bedrockruntime source dir>")
    sdk = sys.argv[1]
    version = os.path.basename(os.path.normpath(sdk)).rsplit("-", 1)[-1]
    src = os.path.join(sdk, "src")
    types_dir = os.path.join(src, "types")
    error_dir = os.path.join(types_dir, "error")
    serde_dir = os.path.join(src, "protocol_serde")

    defs = {}  # name -> (file, kind)
    for directory in (types_dir, error_dir):
        for f in sorted(os.listdir(directory)):
            if f.endswith(".rs") and f != "builders.rs":
                path = os.path.join(directory, f)
                for m in re.finditer(r"^pub (struct|enum) (\w+) \{", open(path).read(), re.M):
                    defs.setdefault(m.group(2), (path, m.group(1)))

    def definition(name):
        """(doc/attribute lines above the item, body lines inside it)."""
        path, _ = defs[name]
        lines = open(path).read().split("\n")
        header = re.compile(r"^pub (struct|enum) " + name + r" \{(\})?$")
        for i, line in enumerate(lines):
            m = header.match(line)
            if m:
                start = i
                while start > 0 and lines[start - 1].lstrip().startswith(("///", "#[")):
                    start -= 1
                # `pub struct Empty {}` has no body lines.
                end = i + 1 if m.group(2) else lines.index("}", i)
                return "\n".join(lines[start:i]) + "\n", "\n".join(lines[i + 1:end]) + "\n"
        die(f"no definition for {name}")

    def serde_keys(name):
        """JSON key -> field or variant name, from the SDK deserializer."""
        base = os.path.basename(defs[name][0])[1:]  # _token_usage.rs -> token_usage.rs
        path = os.path.join(serde_dir, "shape_" + base)
        if not os.path.exists(path):
            return {}
        text = open(path).read()
        keys = {}
        for key, setter in re.findall(r'"(\w+)" =>\s*\{\s*builder\s*=\s*builder\s*\.set_(\w+)\(', text):
            keys[setter] = key
        for key, variant in re.findall(r'"(\w+)" =>\s*\{?\s*Some\(crate::types::' + name + r"::(\w+)\(", text):
            keys[variant] = key
        return keys

    # Reachability from the root.
    seen, stack = set(), [ROOT]
    while stack:
        n = stack.pop()
        if n in seen or n not in defs:
            continue
        seen.add(n)
        stack.extend(set(re.findall(r"crate::types::(\w+)", definition(n)[1])) & set(defs))
    seen |= set(EXCEPTIONS)

    def doc(block, indent):
        text = " ".join(l.strip()[3:].strip() for l in block.splitlines() if l.strip().startswith("///"))
        text = html.unescape(re.sub(r"<[^>]+>", "", text))
        text = re.sub(r"\s+", " ", text).strip()
        if not text:
            return ""
        sentence = re.split(r"(?<=[.!?])\s", text, maxsplit=1)[0]
        return f"{indent}/// {sentence}\n"

    def rust_type(t):
        t = t.strip()
        for wrapper, out in (("::std::option::Option<", "Option<"), ("::std::vec::Vec<", "Vec<")):
            if t.startswith(wrapper):
                return out + rust_type(t[len(wrapper):-1]) + ">"
        if t.startswith("::std::collections::HashMap<"):
            k, v = t[len("::std::collections::HashMap<"):-1].split(",", 1)
            return f"HashMap<{rust_type(k)}, {rust_type(v)}>"
        simple = {
            "::std::string::String": "String",
            "::aws_smithy_types::Document": "Document",
            "::aws_smithy_types::Blob": "Blob",
        }
        if t in simple:
            return simple[t]
        if t.startswith("crate::types::"):
            return t[len("crate::types::"):]
        if re.fullmatch(r"i32|i64|f32|f64|bool", t):
            return t
        die(f"unmapped type {t}")

    def kind_of(name):
        _, body = definition(name)
        if defs[name][1] == "struct":
            return "struct"
        if "sealed_enum_unknown::UnknownVariantValue" in body:
            return "string_enum"
        return "union"

    out = [
        f"// @generated by scripts/generate_converse_stream_types.py from\n"
        f"// aws-sdk-bedrockruntime {version}. Do not edit by hand; re-run the script.\n\n"
        "// `{}` structs stay `{}` (not unit structs) so serde reads them from a JSON `{}`.\n"
        "#![allow(clippy::doc_markdown, clippy::empty_structs_with_brackets, clippy::struct_field_names)]\n\n"
        "use std::collections::HashMap;\n\n"
        "use super::{Blob, Document};\n"
    ]
    for name in sorted(seen - HANDWRITTEN):
        docs, body = definition(name)
        kind = kind_of(name)
        keys = serde_keys(name)
        out.append("\n")
        if kind == "struct":
            out.append(doc(docs, ""))
            out.append("#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]\n")
            out.append(f"pub struct {name} {{\n")
            for fdoc, field, ty in re.findall(r"((?:[ \t]*///.*\n|[ \t]*#\[.*\]\n)*)[ \t]*pub (\w+): (.+),\n", body):
                key = keys.get(field)
                if key is None:
                    die(f"no JSON key for {name}.{field}")
                rt = rust_type(ty)
                attrs = f'rename = "{key}", default'
                if rt.startswith("Option<"):
                    attrs += ', skip_serializing_if = "Option::is_none"'
                out.append(doc(fdoc, "    "))
                out.append(f"    #[serde({attrs})]\n")
                out.append(f"    pub {field}: {rt},\n")
            out.append("}\n")
        elif kind == "string_enum":
            values = dict((v, k) for k, v in re.findall(
                r'"([^"]+)" => ' + name + r"::(\w+),", open(defs[name][0]).read()))
            out.append(f"string_enum! {{\n{doc(docs, '    ')}    {name} {{\n")
            for vdoc, variant in re.findall(r"((?:[ \t]*///.*\n|[ \t]*#\[.*\]\n)*)[ \t]*(\w+),\n", body):
                if variant == "Unknown":
                    continue
                if variant not in values:
                    die(f"no wire value for {name}::{variant}")
                out.append(doc(vdoc, "        "))
                out.append(f'        {variant} = "{values[variant]}",\n')
            out.append("    }\n}\n")
        else:
            out.append(f"union! {{\n{doc(docs, '    ')}    {name} {{\n")
            for vdoc, variant, ty in re.findall(r"((?:[ \t]*///.*\n|[ \t]*#\[.*\]\n)*)[ \t]*(\w+)\((.+)\),\n", body):
                key = keys.get(variant)
                if key is None:
                    die(f"no JSON key for {name}::{variant}")
                out.append(doc(vdoc, "        "))
                out.append(f'        {variant}({rust_type(ty)}) = "{key}",\n')
            out.append("    }\n}\n")

    with open(OUT, "w") as f:
        f.write("".join(out))
    # The same rustfmt invocation CI checks with (.github/workflows/primary.yml),
    # so a regenerated file is already formatted.
    subprocess.run(
        ["rustfmt", "+nightly", "--edition", "2024",
         "--config", "imports_granularity=Crate,group_imports=StdExternalCrate", OUT],
        check=True,
    )
    counts = {}
    for n in seen - HANDWRITTEN:
        counts[kind_of(n)] = counts.get(kind_of(n), 0) + 1
    print(f"wrote {os.path.normpath(OUT)}: {counts}")


if __name__ == "__main__":
    main()
