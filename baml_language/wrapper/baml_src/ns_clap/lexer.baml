// Splits argv into the shapes a command line is made of, the way
// clap_lex does: the `--` escape, `--long[=value]`, a `-abc` cluster of
// short flags, or a plain value. It decides nothing about meaning; the
// parser does that.

class RawArgs {
    items: string[],

    function new(items: string[]) -> RawArgs {
        RawArgs { items: items }
    }

    function cursor(self) -> ArgCursor {
        ArgCursor { index: 0 }
    }

    function next(self, cursor: ArgCursor) -> ParsedArg? {
        let arg = self.peek(cursor);
        if (arg != null) {
            cursor.index += 1;
        }
        arg
    }

    function peek(self, cursor: ArgCursor) -> ParsedArg? {
        match (self.items.at(cursor.index)) {
            null => null,
            let inner: string => ParsedArg { inner: inner },
        }
    }

    // Every argument not read yet. Reads them all.
    function remaining(self, cursor: ArgCursor) -> string[] {
        let rest = self.items.slice(cursor.index, self.items.length());
        cursor.index = self.items.length();
        rest
    }
}

// A position in `RawArgs`. Arguments are read in place: slicing off
// each one would copy the rest every time, which is quadratic in an
// argv of thousands of file names.
class ArgCursor {
    index: int,
}

class ParsedArg {
    inner: string,

    // `--`: everything after it is a value.
    function is_escape(self) -> bool {
        self.inner == "--"
    }

    function is_long(self) -> bool {
        self.inner.starts_with("--") && !self.is_escape()
    }

    // `-x` or `-xyz`. A lone `-` is a value: by convention, stdin.
    function is_short(self) -> bool {
        self.inner.starts_with("-") && !self.inner.starts_with("--") && self.inner.length() > 1
    }

    // Neither a flag nor the escape.
    function is_value(self) -> bool {
        !self.is_long() && !self.is_short() && !self.is_escape()
    }

    function to_long(self) -> LongFlag? {
        if (!self.is_long()) {
            return null;
        }
        let body = self.inner.slice(2, self.inner.length());
        match (body.index_of("=")) {
            null => LongFlag { name: body, value: null },
            let at: int => {
                LongFlag { name: body.slice(0, at), value: body.slice(at + 1, body.length()) }
            },
        }
    }

    function to_short(self) -> ShortFlags? {
        if (!self.is_short()) {
            return null;
        }
        ShortFlags { chars: self.inner.slice(1, self.inner.length()).chars(), index: 0 }
    }
}

// `--name` or `--name=value`.
class LongFlag {
    name: string,
    value: string?,
}

// The flags in a `-abc` cluster, read one at a time. Once a flag takes
// a value, the rest of the cluster is that value: `-ofile`.
class ShortFlags {
    chars: string[],
    index: int,

    function next_flag(self) -> string? {
        let flag = self.chars.at(self.index);
        if (flag != null) {
            self.index += 1;
        }
        flag
    }

    // The rest of the cluster as an attached value; null when nothing is left.
    function next_value(self) -> string? {
        let rest = self.chars.slice(self.index, self.chars.length()).join("");
        self.index = self.chars.length();
        if (rest.is_empty()) {
            null
        } else {
            rest
        }
    }
}
