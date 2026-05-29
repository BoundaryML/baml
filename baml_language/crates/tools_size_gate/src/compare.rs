use crate::{
    config::{GateMetric, Policy},
    measure::ArtifactMeasurement,
};

/// A single policy violation.
pub(crate) struct Violation {
    pub policy_name: String,
    pub metric: String,
    pub actual: String,
    pub limit: String,
    /// How much the actual value exceeds the limit (human-readable).
    pub exceeded_by: String,
}

/// Compare current measurements against baseline + policy. Returns
/// violations. Deltas are measured on `gate` — the metric this artifact
/// is gated on (file size for binaries, gzip for WASM).
pub(crate) fn check_policy(
    current: &ArtifactMeasurement,
    baseline: Option<&ArtifactMeasurement>,
    policy: &Policy,
    gate: GateMetric,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    // The current/baseline value for the gated metric.
    let gated_current = |m: &ArtifactMeasurement| match gate {
        GateMetric::File => m.file_bytes,
        GateMetric::Gzip => m.gzip_bytes,
    };
    let gate_label = gate.label();

    // Absolute file-size limit — the primary gate (actual shipped size).
    if let Some(max) = policy.max_file_bytes {
        if current.file_bytes > max {
            let over = current.file_bytes - max;
            violations.push(Violation {
                metric: "file_bytes".into(),
                policy_name: "max_file_bytes".into(),
                limit: format_bytes(max),
                actual: format_bytes(current.file_bytes),
                exceeded_by: format!("+{}", format_bytes(over)),
            });
        }
    }

    // Absolute gzip limit (optional secondary gate)
    if let Some(max) = policy.max_gzip_bytes {
        if current.gzip_bytes > max {
            let over = current.gzip_bytes - max;
            violations.push(Violation {
                metric: "gzip_bytes".into(),
                policy_name: "max_gzip_bytes".into(),
                limit: format_bytes(max),
                actual: format_bytes(current.gzip_bytes),
                exceeded_by: format!("+{}", format_bytes(over)),
            });
        }
    }

    // Absolute stripped limit
    if let Some(max) = policy.max_stripped_bytes {
        if let Some(stripped) = current.stripped_bytes {
            if stripped > max {
                let over = stripped - max;
                violations.push(Violation {
                    metric: "stripped_bytes".into(),
                    policy_name: "max_stripped_bytes".into(),
                    limit: format_bytes(max),
                    actual: format_bytes(stripped),
                    exceeded_by: format!("+{}", format_bytes(over)),
                });
            }
        }
    }

    // Delta checks require baseline. Deltas are measured on the gated
    // metric (file size for binaries, gzip for WASM).
    if let Some(base) = baseline {
        let cur = gated_current(current) as i64;
        let prev = gated_current(base) as i64;

        // Absolute delta
        if let Some(max_delta) = policy.max_delta_bytes {
            let delta = cur - prev;
            if delta > max_delta {
                let over = delta - max_delta;
                violations.push(Violation {
                    metric: format!("{gate_label}_delta"),
                    policy_name: "max_delta_bytes".into(),
                    limit: format_delta_bytes(max_delta),
                    actual: format_delta_bytes(delta),
                    exceeded_by: format!("+{}", format_bytes(over.unsigned_abs())),
                });
            }
        }

        // Percentage delta
        if let Some(max_pct) = policy.max_delta_pct {
            if prev > 0 {
                let delta_pct = ((cur - prev) as f64 / prev as f64) * 100.0;
                if delta_pct > max_pct {
                    let over = delta_pct - max_pct;
                    violations.push(Violation {
                        metric: format!("{gate_label}_delta_pct"),
                        policy_name: "max_delta_pct".into(),
                        limit: format!("{max_pct:.1}%"),
                        actual: format!("{delta_pct:+.1}%"),
                        exceeded_by: format!("+{over:.1}pp"),
                    });
                }
            }
        }
    }

    violations
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_delta_bytes(bytes: i64) -> String {
    let abs = bytes.unsigned_abs();
    let sign = if bytes >= 0 { "+" } else { "-" };
    if abs >= 1_000_000 {
        format!("{sign}{:.1} MB", abs as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{sign}{:.1} KB", abs as f64 / 1_000.0)
    } else {
        format!("{sign}{abs} B")
    }
}
