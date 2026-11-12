// Text helpers for help rendering and optional values.
class Text {
    function pad_to(text: string, width: int) -> string {
        `${text}${" ".repeat(width - text.length())}`
    }

    function non_empty(text: string) -> string? {
        if (text.is_empty()) {
            null
        } else {
            text
        }
    }

    function join_present(parts: (string | null)[], separator: string) -> string {
        parts
            .filter_map((part: string?) -> string? {
                part
            })
            .join(separator)
    }

    function max_length(texts: string[]) -> int {
        texts.reduce(
            (widest: int, text: string) -> int {
                if (text.length() > widest) {
                    text.length()
                } else {
                    widest
                }
            },
            0,
        )
    }

    function parse_int_or_null(text: string) -> int? {
        int.parse(text) catch (e) {
            baml.errors.ParseError => null,
        }
    }
}
