use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use web_time::Instant;

#[derive(serde::Deserialize)]
pub(crate) struct Color {
    pub token: String,
}

pub(crate) static PALETTE: LazyLock<Vec<Color>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("rainbow.json")).expect("valid rainbow palette")
});

const FRAME: Duration = Duration::from_millis(42);

fn phase(elapsed: Duration) -> Option<usize> {
    let step = elapsed.as_millis() / FRAME.as_millis();
    let count = PALETTE.len() as u128;
    (step < count).then_some(((count - step % count) % count) as usize)
}

#[derive(Default)]
pub(crate) struct Rainbow {
    pub enabled: bool,
    pub refresh: bool,
    documents: HashMap<PathBuf, Option<Instant>>,
    due: Option<Instant>,
    sequence: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RainbowView {
    enabled: bool,
    phases: HashMap<PathBuf, usize>,
}

impl RainbowView {
    pub(crate) fn phase(&self, path: &Path) -> Option<usize> {
        self.enabled
            .then(|| self.phases.get(path).copied().unwrap_or(0))
    }
}

impl Rainbow {
    pub(crate) fn view(&self, now: Instant) -> RainbowView {
        RainbowView {
            enabled: self.enabled,
            phases: self
                .documents
                .iter()
                .filter_map(|(path, start)| {
                    start
                        .and_then(|start| phase(now.saturating_duration_since(start)))
                        .map(|phase| (path.clone(), phase))
                })
                .collect(),
        }
    }

    pub(crate) fn observe(&mut self, path: PathBuf, now: Instant) {
        if !self.enabled || !self.refresh || self.documents.contains_key(&path) {
            return;
        }
        self.documents.insert(path, Some(now));
        self.due.get_or_insert(now + FRAME);
    }

    pub(crate) fn settle(&mut self, path: &Path) {
        if let Some(start) = self.documents.get_mut(path) {
            *start = None;
        }
        if self.documents.values().all(Option::is_none) {
            self.due = None;
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.due
    }

    pub(crate) fn tick(&mut self, now: Instant) -> Option<lsp_server::Request> {
        if self.due.is_none_or(|due| due > now) {
            return None;
        }
        for start in self.documents.values_mut() {
            if start.is_some_and(|start| phase(now.saturating_duration_since(start)).is_none()) {
                *start = None;
            }
        }
        self.due = self
            .documents
            .values()
            .any(Option::is_some)
            .then_some(now + FRAME);
        self.sequence += 1;
        Some(lsp_server::Request::new(
            format!("baml-rainbow-{}", self.sequence).into(),
            <lsp_types::request::SemanticTokensRefresh as lsp_types::request::Request>::METHOD
                .to_owned(),
            (),
        ))
    }
}

pub(crate) fn token_type(character: u32, length: u32, phase: usize) -> u32 {
    let color = (character as usize * PALETTE.len() / length as usize + phase) % PALETTE.len();
    u32::try_from(baml_ide::TOKEN_TYPES.len() + color).expect("small token legend")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_scroll_settles_after_one_second_without_replaying() {
        let mut rainbow = Rainbow {
            enabled: true,
            refresh: true,
            ..Rainbow::default()
        };
        let path = PathBuf::from("/magic.baml");
        let now = Instant::now();
        rainbow.observe(path.clone(), now);
        assert_eq!(rainbow.view(now).phase(&path), Some(0));
        assert!(rainbow.tick(now + FRAME).is_some());
        assert_eq!(
            rainbow.view(now + FRAME).phase(&path),
            Some(PALETTE.len() - 1)
        );
        let end = now + Duration::from_millis(1008);
        assert!(rainbow.tick(end).is_some());
        assert_eq!(rainbow.view(end).phase(&path), Some(0));
        assert!(rainbow.next_deadline().is_none());
        rainbow.settle(&path);
        rainbow.observe(path, end);
        assert!(rainbow.next_deadline().is_none());
    }

    #[test]
    fn opt_in_and_refresh_support_are_independent() {
        let path = PathBuf::from("/magic.baml");
        let now = Instant::now();
        let mut rainbow = Rainbow::default();
        rainbow.observe(path.clone(), now);
        assert_eq!(rainbow.view(now).phase(&path), None);
        rainbow.enabled = true;
        rainbow.observe(path.clone(), now);
        assert_eq!(rainbow.view(now).phase(&path), Some(0));
        assert!(rainbow.next_deadline().is_none());
        rainbow.refresh = true;
        rainbow.observe(path.clone(), now);
        assert!(rainbow.next_deadline().is_some());
        rainbow.settle(&path);
        assert!(rainbow.next_deadline().is_none());
    }
}
