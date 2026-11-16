enum Metric {
    Cognitive,
    Cyclomatic,
    NPath,
    Nesting,
    Exits,
}

/// The metrics in the order the comment shows them.
const ALL_METRICS: Metric[] = [Metric.Cognitive, Metric.Cyclomatic, Metric.NPath, Metric.Nesting, Metric.Exits];

/// A metric's name inside a sentence.
function metric_word(metric: Metric) -> string {
    match (metric) {
        Metric.NPath => "NPath",
        _ => metric.to_string().to_lower_case(),
    }
}

/// Identifies a function across the base and head reports.
class FunctionKey {
    name: string,
    file: string,

    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering {
            if (self.name != other.name) {
                self.name.cmp(other.name)
            } else {
                self.file.cmp(other.file)
            }
        }
    }
}

/// One function in the report of `baml codequality --output-format json`.
/// Fields of the report not listed here are ignored.
class FunctionReport {
    name: string,
    file: string,
    start_line: int,
    end_line: int,
    cognitive: bigint,
    cyclomatic: bigint,
    npath: bigint,
    nesting: bigint,
    exits: bigint,

    function key(self) -> FunctionKey {
        FunctionKey { name: self.name, file: self.file }
    }

    function value(self, metric: Metric) -> bigint {
        match (metric) {
            Metric.Cognitive => self.cognitive,
            Metric.Cyclomatic => self.cyclomatic,
            Metric.NPath => self.npath,
            Metric.Nesting => self.nesting,
            Metric.Exits => self.exits,
        }
    }
}

/// One file in the report.
class FileReport {
    file: string,
    lines: bigint,
}

class Report {
    functions: FunctionReport[],
    files: FileReport[],

    /// Decodes a report, with Windows paths turned into forward-slash ones.
    function parse(text: string) -> Self throws baml.json.ParseError | baml.json.DecodeError {
        let report = baml.json.decode<Report>(text);
        Report {
            functions: report.functions.map((f) -> {
                FunctionReport { ...f, file: f.file.replace_all("\\", "/") }
            }),
            files: report.files.map((f) -> {
                FileReport { ...f, file: f.file.replace_all("\\", "/") }
            }),
        }
    }

    /// Functions by key, keeping the first of any duplicates.
    function functions_by_key(self) -> map<FunctionKey, FunctionReport> {
        let by_key: map<FunctionKey, FunctionReport> = {};
        for (let f in self.functions) {
            by_key.get_or_insert(f.key(), f);
        }
        by_key
    }

    function summary(self) -> Summary {
        let sums: map<Metric, bigint> = {};
        for (let m in ALL_METRICS) {
            sums.set(
                m,
                self.functions.reduce(
                    (total, f) -> {
                        total + f.value(m)
                    },
                    0n,
                ),
            );
        }
        Summary {
            functions: self.functions.length(),
            lines: self.files.reduce(
                (total, f) -> {
                    total + f.lines
                },
                0n,
            ),
            sums: sums,
        }
    }
}
