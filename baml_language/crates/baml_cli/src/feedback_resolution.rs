//! Bounded, anonymous resolution lookups. Delivery records are never rewritten.
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Read,
    time::{Duration, Instant},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DAY: u64 = 86400;
const BATCH: usize = 100;
const URL: &str = match option_env!("BAML_FEEDBACK_SUPABASE_URL") {
    Some(value) => value,
    None => "https://igraichzcidsylvzkjlc.supabase.co",
};
// Public publishable key only. No runtime service-role environment fallback.
const KEY: Option<&str> = option_env!("BAML_FEEDBACK_PUBLISHABLE_KEY");

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct IssueResolution {
    pub issue_id: String,
    pub state: String,
    pub fixed_in: Option<String>,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Cache {
    last_attempt: u64,
    last_notice: u64,
    next: usize,
    pub reports: BTreeMap<Uuid, Vec<IssueResolution>>,
}
#[derive(Deserialize)]
struct Row {
    report_id: Uuid,
    issue_id: Option<String>,
    state: Option<String>,
    fixed_in: Option<String>,
}

pub(crate) fn stable_version(value: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || !p.bytes().all(|b| b.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
        })
    {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

pub(crate) fn resolution(issues: &[IssueResolution], installed: &str) -> &'static str {
    if issues.is_empty() {
        return "triaging";
    }
    let all_fixed = issues
        .iter()
        .all(|i| i.state == "merged" && i.fixed_in.as_deref().and_then(stable_version).is_some());
    if !all_fixed {
        return "in_progress";
    }
    if stable_version(installed).is_some_and(|current| {
        issues.iter().all(|i| {
            i.fixed_in
                .as_deref()
                .and_then(stable_version)
                .is_some_and(|fixed| current >= fixed)
        })
    }) {
        "resolved"
    } else {
        "fixed"
    }
}

fn cache_path() -> std::path::PathBuf {
    baml_release::baml_home().join("feedback-resolutions.json")
}
pub(crate) fn load() -> Cache {
    std::fs::read(cache_path())
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}
fn save(cache: &Cache) -> Result<()> {
    let path = cache_path();
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    crate::auth::write_owner_only(&tmp, &serde_json::to_string(cache)?)?;
    // All writers hold the separate lock; readers see either complete version.
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
fn apply(cache: &mut Cache, ids: &[Uuid], rows: Vec<Row>) -> Result<()> {
    let mut updates: BTreeMap<Uuid, Vec<IssueResolution>> =
        ids.iter().map(|id| (*id, Vec::new())).collect();
    for row in rows {
        let Some(issues) = updates.get_mut(&row.report_id) else {
            anyhow::bail!("unexpected report ID");
        };
        if let Some(id) = row.issue_id {
            if id.len() > 100 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
                anyhow::bail!("invalid issue ID");
            }
            let state = row.state.unwrap_or_default();
            if ![
                "open",
                "awaiting_approval",
                "approved",
                "in_progress",
                "merged",
                "closed",
                "wont_fix",
                "duplicate",
            ]
            .contains(&state.as_str())
            {
                anyhow::bail!("unknown issue state");
            }
            if row
                .fixed_in
                .as_deref()
                .is_some_and(|v| stable_version(v).is_none())
            {
                anyhow::bail!("invalid release version");
            }
            if !issues.iter().any(|i| i.issue_id == id) {
                issues.push(IssueResolution {
                    issue_id: id,
                    state,
                    fixed_in: row.fixed_in,
                });
            }
        }
    }
    cache.reports.extend(updates);
    Ok(())
}

/// At most 1.5 seconds total, including all batches; a later poll resumes at the
/// next batch. Missing configuration and network errors preserve cached results.
pub(crate) fn refresh(ids: &[Uuid], force: bool, notice: bool, installed: &str) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let path = cache_path();
    std::fs::create_dir_all(path.parent().expect("cache parent"))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    if lock.try_lock().is_err() {
        return Ok(());
    }
    let mut cache = load();
    let now = crate::auth::now_unix();
    if force || now.saturating_sub(cache.last_attempt) >= DAY {
        cache.last_attempt = now;
        if let Some(key) = KEY.filter(|k| k.starts_with("sb_publishable_") && k.len() < 256) {
            let base = reqwest::Url::parse(URL)?;
            anyhow::ensure!(
                base.scheme() == "https"
                    && base.username().is_empty()
                    && base.password().is_none()
                    && base.path() == "/"
                    && base.query().is_none()
                    && base.fragment().is_none(),
                "invalid status origin"
            );
            let client = reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let started = Instant::now();
            let budget = Duration::from_millis(1500);
            let start = cache.next % ids.len();
            let ordered: Vec<_> = ids[start..]
                .iter()
                .chain(ids[..start].iter())
                .copied()
                .collect();
            for batch in ordered.chunks(BATCH) {
                let Some(left) = budget.checked_sub(started.elapsed()) else {
                    break;
                };
                let result = (|| -> Result<Vec<Row>> {
                    let response = client
                        .post(base.join("rest/v1/rpc/feedback_resolutions")?)
                        .header("apikey", key)
                        .json(&serde_json::json!({"report_ids":batch}))
                        .timeout(left)
                        .send()?
                        .error_for_status()?;
                    let mut bytes = Vec::new();
                    response.take(1_000_001).read_to_end(&mut bytes)?;
                    anyhow::ensure!(bytes.len() <= 1_000_000, "oversized resolution response");
                    Ok(serde_json::from_slice(&bytes)?)
                })();
                match result {
                    Ok(rows) => {
                        if apply(&mut cache, batch, rows).is_err() {
                            break;
                        }
                        cache.next = (cache.next + batch.len()) % ids.len();
                    }
                    Err(_) => break,
                }
            }
        }
    }
    cache.reports.retain(|id, _| ids.contains(id));
    let mut messages = Vec::new();
    if notice && now.saturating_sub(cache.last_notice) >= DAY {
        for (id, issues) in &cache.reports {
            for issue in issues {
                if issue.state == "merged"
                    && let Some(version) = &issue.fixed_in
                    && let Some(fixed) = stable_version(version)
                    && stable_version(installed).is_none_or(|current| current < fixed)
                {
                    messages.push(format!("Your feedback {} (issue {}) was fixed in BAML {}. Update using baml toolchain update.", &id.to_string()[..8], issue.issue_id, version));
                }
            }
        }
        if !messages.is_empty() {
            cache.last_notice = now;
        }
    }
    save(&cache)?;
    for message in messages {
        crate::reporter::print_warning(message);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn issue(id: &str, state: &str, fixed: Option<&str>) -> IssueResolution {
        IssueResolution {
            issue_id: id.into(),
            state: state.into(),
            fixed_in: fixed.map(str::to_owned),
        }
    }
    #[test]
    fn resolution_requires_every_linked_issue_and_suppresses_after_upgrade() {
        let fixed = issue("i1", "merged", Some("0.19.0"));
        assert_eq!(resolution(&[], "0.18.0"), "triaging");
        assert_eq!(
            resolution(&[fixed.clone(), issue("i2", "approved", None)], "0.18.0"),
            "in_progress"
        );
        assert_eq!(resolution(&[fixed.clone()], "0.18.0"), "fixed");
        assert_eq!(resolution(&[fixed.clone()], "0.19.0"), "resolved");
        assert_eq!(resolution(&[fixed], "0.19.0-nightly.1"), "fixed");
    }
    #[test]
    fn invalid_or_unrelated_response_cannot_partially_update_cache() {
        let id = Uuid::new_v4();
        let mut cache = Cache::default();
        cache
            .reports
            .insert(id, vec![issue("original", "approved", None)]);
        assert!(
            apply(
                &mut cache,
                &[id],
                vec![Row {
                    report_id: Uuid::new_v4(),
                    issue_id: None,
                    state: None,
                    fixed_in: None
                }]
            )
            .is_err()
        );
        assert_eq!(cache.reports[&id][0].issue_id, "original");
        assert!(
            apply(
                &mut cache,
                &[id],
                vec![Row {
                    report_id: id,
                    issue_id: Some("i1".into()),
                    state: Some("merged".into()),
                    fixed_in: Some("0.19.0\nunsafe".into())
                }]
            )
            .is_err()
        );
        assert_eq!(cache.reports[&id][0].issue_id, "original");
    }
    #[test]
    fn bounded_response_updates_only_its_batch_and_handles_missing_records() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut cache = Cache::default();
        cache.reports.insert(b, vec![issue("i2", "approved", None)]);
        apply(&mut cache, &[a], vec![]).unwrap();
        assert!(cache.reports[&a].is_empty());
        assert_eq!(cache.reports[&b].len(), 1);
    }
}
