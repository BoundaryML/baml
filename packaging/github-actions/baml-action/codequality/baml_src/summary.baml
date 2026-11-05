/// What the comment needs of the project as a whole.
class Summary {
    functions: int,
    lines: bigint,
    /// Each metric summed over every function.
    sums: map<Metric, bigint>,

    /// The average per function, exactly; null for a project with no functions.
    function average(self, metric: Metric) -> Ratio? {
        if (!self.functions) {
            return null;
        }
        Ratio.new(self.sums.get(metric) ?? 0n, self.functions + 0n)
    }
}

/// An exact fraction in lowest terms, because NPath can be too large for a
/// float. Lowest terms make structural equality agree with `cmp`.
class Ratio {
    numerator: bigint,
    denominator: bigint,

    function new(
        numerator: bigint,
        denominator: bigint,
    ) -> Self throws baml.errors.InvalidArgument {
        if (denominator < 1n) {
            throw baml.errors.InvalidArgument { message: `ratio denominator ${denominator} is not positive` }
        }
        let divisor = gcd(numerator.abs(), denominator);
        Ratio { numerator: numerator / divisor, denominator: denominator / divisor }
    }

    /// The ratio as a percentage, rounded half up.
    function percent(self) -> bigint {
        (self.numerator * 100n + self.denominator / 2n) / self.denominator
    }

    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering {
            (self.numerator * other.denominator).cmp(other.numerator * self.denominator)
        }
    }
}

function gcd(a: bigint, b: bigint) -> bigint {
    if (!b) {
        return a;
    }
    gcd(b, a % b)
}

/// How `count` abbreviates numbers of 7 to 15 digits.
class Scale {
    exponent: int,
    suffix: string,
}

const SCALES: Scale[] = [
    Scale { exponent: 6, suffix: "M" },
    Scale { exponent: 9, suffix: "B" },
    Scale { exponent: 12, suffix: "T" },
];

/// `n` for a reader, as `baml codequality` writes it: digits grouped by
/// thousands up to a million, then one decimal and a suffix (`2.7M`,
/// `273.0B`), then scientific notation past a thousand trillion.
function count(n: bigint) -> string {
    let digits = n.to_string();
    let length = digits.length();
    if (length <= 3) {
        return digits;
    }
    if (length <= 6) {
        return `${digits.slice(0, length - 3)},${digits.slice(length - 3, length)}`;
    }
    let scale = SCALES.find((s) -> {
        length <= s.exponent + 3
    });
    if (scale) {
        let tenths = tenths_of(n, scale.exponent);
        return `${tenths / 10n}.${tenths % 10n}${scale.suffix}`;
    }
    let tenths = tenths_of(n, length - 1);
    // Rounding can carry into a new digit: `9.96e18` reads `1.0e19`.
    if (tenths >= 100n) {
        return `1.0e${length}`;
    }
    `${tenths / 10n}.${tenths % 10n}e${length - 1}`
}

/// `n / 10^exponent` in tenths, rounded half up.
function tenths_of(n: bigint, exponent: int) -> bigint {
    let unit = 10n.pow(exponent - 1 + 0n);
    (n + unit / 2n) / unit
}

function plural(count: int, word: string) -> string {
    match (count) {
        1 => `1 ${word}`,
        _ => `${count} ${word}s`,
    }
}
