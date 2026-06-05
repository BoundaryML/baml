"""Convert a subset of Markdown into Notion block objects.

Pure functions, no Notion API calls and no third-party deps. Handles the
Markdown our issue descriptions actually use: ``#``/``##``/``###`` headings,
``-``/``*`` bullets, ``1.`` numbered lists, fenced ```` ``` ```` code blocks
(with an optional language), ``---`` dividers, and paragraphs -- with
``**bold**`` and `` `inline code` `` parsed into Notion ``rich_text``
annotations.

Notion limits respected here: each ``rich_text`` ``text.content`` is chunked to
<= 2000 chars. The 100-children-per-request limit is the caller's to honor --
``markdown_to_blocks`` returns the full list and the Notion client batches it.
"""

from __future__ import annotations

import re

TEXT_LIMIT = 2000

# Notion's code-block language enum is closed; an unknown value is a 400. Map the
# fences we actually see and fall back to "plain text" for anything else.
_NOTION_LANGS = {
    "bash", "c", "c++", "c#", "css", "diff", "docker", "go", "graphql", "html",
    "java", "javascript", "json", "kotlin", "less", "lua", "makefile", "markdown",
    "matlab", "mermaid", "objective-c", "ocaml", "perl", "php", "plain text",
    "powershell", "protobuf", "python", "ruby", "rust", "sass", "scala", "scss",
    "shell", "sql", "swift", "typescript", "xml", "yaml",
}
_LANG_ALIASES = {
    "sh": "shell", "zsh": "shell", "js": "javascript", "ts": "typescript",
    "py": "python", "rs": "rust", "yml": "yaml", "text": "plain text",
    "txt": "plain text", "": "plain text", "baml": "plain text", "console": "shell",
}

_INLINE = re.compile(r"(\*\*.+?\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))")
_LINK = re.compile(r"^\[([^\]]+)\]\(([^)]+)\)$")


def _norm_lang(lang: str) -> str:
    """Normalize a fence language to a Notion-supported code language.

    Args:
        lang: The raw language tag from a Markdown fence (may be empty).

    Returns:
        A language value Notion accepts, defaulting to ``"plain text"``.
    """
    low = (lang or "").strip().lower()
    low = _LANG_ALIASES.get(low, low)
    return low if low in _NOTION_LANGS else "plain text"


def _split(text: str, limit: int = TEXT_LIMIT) -> list[str]:
    """Split a string into <=limit-char pieces.

    Args:
        text: The text to split.
        limit: Maximum length of each piece.

    Returns:
        The pieces in order; always at least one (possibly empty) piece.
    """
    if not text:
        return [""]
    return [text[i : i + limit] for i in range(0, len(text), limit)]


def _rich_text(text: str) -> list[dict]:
    """Parse ``**bold**`` and `` `inline code` `` into Notion rich_text segments.

    Each segment's content is chunked to the 2000-char limit.

    Args:
        text: A single line/paragraph of Markdown-ish text.

    Returns:
        A non-empty list of Notion rich_text objects.
    """
    out: list[dict] = []
    for part in _INLINE.split(text):
        if not part:
            continue
        ann: dict = {}
        link: str | None = None
        content = part
        m = _LINK.match(part)
        if m:
            content, link = m.group(1), m.group(2)
        elif part.startswith("**") and part.endswith("**") and len(part) >= 4:
            ann, content = {"bold": True}, part[2:-2]
        elif part.startswith("`") and part.endswith("`") and len(part) >= 2:
            ann, content = {"code": True}, part[1:-1]
        for chunk in _split(content):
            text_obj: dict = {"content": chunk}
            if link:
                text_obj["link"] = {"url": link}
            seg: dict = {"type": "text", "text": text_obj}
            if ann:
                seg["annotations"] = ann
            out.append(seg)
    return out or [{"type": "text", "text": {"content": ""}}]


def _block(btype: str, rich: list[dict]) -> dict:
    """Wrap rich_text in a Notion block of the given type.

    Args:
        btype: The Notion block type (e.g. ``"paragraph"``, ``"heading_2"``).
        rich: The rich_text segments to carry.

    Returns:
        A Notion block object.
    """
    return {"object": "block", "type": btype, btype: {"rich_text": rich}}


def code_block(text: str, language: str = "plain text") -> dict:
    """Build a Notion code block, chunking content to the 2000-char limit.

    Args:
        text: The code/verbatim text.
        language: Desired language; normalized to a Notion-supported value.

    Returns:
        A Notion ``code`` block object.
    """
    rich = [{"type": "text", "text": {"content": c}} for c in _split(text or "")]
    return {
        "object": "block",
        "type": "code",
        "code": {"rich_text": rich, "language": _norm_lang(language)},
    }


def markdown_to_blocks(md: str) -> list[dict]:
    """Convert a subset of Markdown into a list of Notion block objects.

    Args:
        md: The Markdown source.

    Returns:
        The Notion blocks in document order (caller batches at <=100/request).
    """
    blocks: list[dict] = []
    lines = (md or "").split("\n")
    i = 0
    while i < len(lines):
        stripped = lines[i].strip()

        # Fenced code block.
        fence = re.match(r"^```(.*)$", stripped)
        if fence:
            lang = fence.group(1).strip()
            body: list[str] = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                body.append(lines[i])
                i += 1
            i += 1  # skip the closing fence
            blocks.append(code_block("\n".join(body), lang))
            continue

        if not stripped:
            i += 1
            continue

        if stripped in ("---", "***", "___"):
            blocks.append({"object": "block", "type": "divider", "divider": {}})
            i += 1
            continue

        heading = re.match(r"^(#{1,6})\s+(.*)$", stripped)
        if heading:
            level = min(len(heading.group(1)), 3)
            blocks.append(_block(f"heading_{level}", _rich_text(heading.group(2).strip())))
            i += 1
            continue

        bullet = re.match(r"^[-*]\s+(.*)$", stripped)
        if bullet:
            blocks.append(_block("bulleted_list_item", _rich_text(bullet.group(1).strip())))
            i += 1
            continue

        numbered = re.match(r"^\d+\.\s+(.*)$", stripped)
        if numbered:
            blocks.append(_block("numbered_list_item", _rich_text(numbered.group(1).strip())))
            i += 1
            continue

        # Paragraph: gather consecutive plain lines until a blank/special line.
        para = [stripped]
        i += 1
        while i < len(lines):
            nxt = lines[i].strip()
            if (
                not nxt
                or nxt.startswith("#")
                or nxt.startswith("```")
                or nxt in ("---", "***", "___")
                or re.match(r"^[-*]\s+", nxt)
                or re.match(r"^\d+\.\s+", nxt)
            ):
                break
            para.append(nxt)
            i += 1
        blocks.append(_block("paragraph", _rich_text(" ".join(para))))

    return blocks
