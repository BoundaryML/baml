//! TUI Visualization for GEPA Optimization
//!
//! Provides a real-time terminal user interface for visualizing the GEPA
//! optimization process, showing trials, candidates, and their metrics.

use std::{
    collections::HashMap,
    io::{self, Stdout},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame, Terminal,
};

use super::{
    candidate::{Candidate, CandidateMethod, CandidateScores},
    storage::{ObjectiveConfig, OptimizationStorage},
};

/// Colors for the TUI
const ACCENT_COLOR: Color = Color::Rgb(142, 36, 170); // Purple
const SELECTED_BG: Color = Color::Rgb(60, 60, 80);
const HEADER_COLOR: Color = Color::Cyan;
const SCORE_GOOD: Color = Color::Green;
const SCORE_MED: Color = Color::Yellow;
const SCORE_BAD: Color = Color::Red;

/// Represents a selectable item in the trial/candidate tree
#[derive(Clone, Debug)]
pub enum TreeItem {
    /// A trial (iteration) header
    Trial {
        iteration: usize,
        candidate_count: usize,
    },
    /// A candidate within a trial
    Candidate { candidate_id: usize },
}

/// Main application state for the TUI
pub struct App {
    /// All candidates loaded from storage
    candidates: Vec<Candidate>,
    /// Tree items for the left panel (trials and candidates)
    tree_items: Vec<TreeItem>,
    /// Currently selected index in the tree
    selected_index: usize,
    /// List state for ratatui
    list_state: ListState,
    /// Scroll position for the prompt preview
    prompt_scroll: u16,
    /// Whether the app should quit
    should_quit: bool,
    /// Storage path for display
    storage_path: String,
    /// Optimization function name
    function_name: String,
    /// Maps candidate ID to candidate index in the Vec
    id_to_index: HashMap<usize, usize>,
    /// Configured objectives from the optimization run
    objectives: Vec<ObjectiveConfig>,
    /// Pareto frontier candidate IDs
    pareto_frontier: Vec<usize>,
}

impl App {
    /// Create a new App from an optimization storage directory
    pub fn from_storage(storage: &OptimizationStorage) -> Result<Self> {
        let candidates = storage
            .load_candidates()
            .context("Failed to load candidates")?;

        let config = storage.load_config().ok();
        let function_name = config
            .as_ref()
            .map(|c| c.function_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let objectives = config
            .as_ref()
            .map(|c| c.objectives.clone())
            .unwrap_or_default();

        // Load Pareto frontier from state or results
        let pareto_frontier = storage
            .load_state()
            .ok()
            .flatten()
            .map(|s| s.pareto_frontier_indices)
            .or_else(|| {
                storage
                    .load_results()
                    .ok()
                    .map(|r| r.pareto_frontier.iter().map(|p| p.id).collect())
            })
            .unwrap_or_default();

        let storage_path = storage.run_dir().display().to_string();

        Self::from_candidates_with_config(candidates, function_name, storage_path, objectives, pareto_frontier)
    }

    /// Create a new App from a list of candidates with configuration
    pub fn from_candidates_with_config(
        candidates: Vec<Candidate>,
        function_name: String,
        storage_path: String,
        objectives: Vec<ObjectiveConfig>,
        pareto_frontier: Vec<usize>,
    ) -> Result<Self> {
        // Build ID to index map
        let id_to_index: HashMap<usize, usize> = candidates
            .iter()
            .enumerate()
            .map(|(idx, c)| (c.id, idx))
            .collect();

        // Build tree structure grouped by iteration
        let mut tree_items = Vec::new();
        let mut candidates_by_iteration: HashMap<usize, Vec<usize>> = HashMap::new();

        for candidate in &candidates {
            candidates_by_iteration
                .entry(candidate.iteration)
                .or_default()
                .push(candidate.id);
        }

        // Sort iterations
        let mut iterations: Vec<usize> = candidates_by_iteration.keys().copied().collect();
        iterations.sort();

        for iteration in iterations {
            let cand_ids = candidates_by_iteration.get(&iteration).unwrap();
            tree_items.push(TreeItem::Trial {
                iteration,
                candidate_count: cand_ids.len(),
            });
            for &cand_id in cand_ids {
                tree_items.push(TreeItem::Candidate {
                    candidate_id: cand_id,
                });
            }
        }

        let mut list_state = ListState::default();
        if !tree_items.is_empty() {
            list_state.select(Some(0));
        }

        Ok(Self {
            candidates,
            tree_items,
            selected_index: 0,
            list_state,
            prompt_scroll: 0,
            should_quit: false,
            storage_path,
            function_name,
            id_to_index,
            objectives,
            pareto_frontier,
        })
    }

    /// Get the currently selected candidate, if any
    fn selected_candidate(&self) -> Option<&Candidate> {
        match self.tree_items.get(self.selected_index)? {
            TreeItem::Trial { .. } => {
                // If a trial is selected, show the first candidate in that trial
                if self.selected_index + 1 < self.tree_items.len() {
                    if let TreeItem::Candidate { candidate_id } =
                        &self.tree_items[self.selected_index + 1]
                    {
                        return self
                            .id_to_index
                            .get(candidate_id)
                            .and_then(|&idx| self.candidates.get(idx));
                    }
                }
                None
            }
            TreeItem::Candidate { candidate_id } => self
                .id_to_index
                .get(candidate_id)
                .and_then(|&idx| self.candidates.get(idx)),
        }
    }

    /// Check if a candidate is on the Pareto frontier
    fn is_pareto(&self, candidate_id: usize) -> bool {
        self.pareto_frontier.contains(&candidate_id)
    }

    /// Move selection up
    fn select_previous(&mut self) {
        if self.tree_items.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.list_state.select(Some(self.selected_index));
            self.prompt_scroll = 0;
        }
    }

    /// Move selection down
    fn select_next(&mut self) {
        if self.tree_items.is_empty() {
            return;
        }
        if self.selected_index < self.tree_items.len() - 1 {
            self.selected_index += 1;
            self.list_state.select(Some(self.selected_index));
            self.prompt_scroll = 0;
        }
    }

    /// Scroll prompt view up
    fn scroll_prompt_up(&mut self) {
        self.prompt_scroll = self.prompt_scroll.saturating_sub(3);
    }

    /// Scroll prompt view down
    fn scroll_prompt_down(&mut self) {
        self.prompt_scroll = self.prompt_scroll.saturating_add(3);
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => {
                for _ in 0..5 {
                    self.select_previous();
                }
            }
            KeyCode::PageDown => {
                for _ in 0..5 {
                    self.select_next();
                }
            }
            KeyCode::Home => {
                self.selected_index = 0;
                self.list_state.select(Some(0));
                self.prompt_scroll = 0;
            }
            KeyCode::End => {
                if !self.tree_items.is_empty() {
                    self.selected_index = self.tree_items.len() - 1;
                    self.list_state.select(Some(self.selected_index));
                    self.prompt_scroll = 0;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => self.scroll_prompt_up(),
            KeyCode::Right | KeyCode::Char('l') => self.scroll_prompt_down(),
            KeyCode::Char('[') => self.scroll_prompt_up(),
            KeyCode::Char(']') => self.scroll_prompt_down(),
            _ => {}
        }
    }

    /// Get the metric value for a given objective name from scores
    fn get_objective_value(objective: &ObjectiveConfig, scores: &CandidateScores) -> f64 {
        match objective.name.as_str() {
            "accuracy" => scores.test_pass_rate,
            "tokens" => scores.avg_prompt_tokens + scores.avg_completion_tokens,
            "prompt_tokens" => scores.avg_prompt_tokens,
            "completion_tokens" => scores.avg_completion_tokens,
            "latency" => scores.avg_latency_ms,
            name if name.starts_with("check:") => {
                let check_name = &name[6..];
                scores.check_scores.get(check_name).copied().unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }
}

/// Render the UI
fn render_ui(frame: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(2), // Footer/help
        ])
        .split(frame.area());

    render_header(frame, app, main_chunks[0]);

    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Left panel (trials/candidates)
            Constraint::Percentage(70), // Right panel (details)
        ])
        .split(main_chunks[1]);

    render_tree_panel(frame, app, content_chunks[0]);
    render_details_panel(frame, app, content_chunks[1]);
    render_footer(frame, main_chunks[2]);
}

/// Render the header
fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = format!(" GEPA Optimization Viewer - {} ", app.function_name);
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_COLOR));

    // Show objectives in header
    let objectives_str = if app.objectives.is_empty() {
        "default".to_string()
    } else {
        app.objectives
            .iter()
            .map(|o| format!("{}={:.0}%", o.name, o.weight * 100.0))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let stats = format!(
        "Candidates: {} | Pareto: {} | Objectives: {}",
        app.candidates.len(),
        app.pareto_frontier.len(),
        objectives_str
    );
    let paragraph = Paragraph::new(stats)
        .style(Style::default().fg(Color::Gray))
        .block(block);

    frame.render_widget(paragraph, area);
}

/// Render the left panel with trial/candidate tree
fn render_tree_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(" Trials & Candidates ")
        .title_style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let items: Vec<ListItem> = app
        .tree_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == app.selected_index;
            match item {
                TreeItem::Trial {
                    iteration,
                    candidate_count,
                } => {
                    let style = if is_selected {
                        Style::default()
                            .fg(ACCENT_COLOR)
                            .add_modifier(Modifier::BOLD)
                            .bg(SELECTED_BG)
                    } else {
                        Style::default()
                            .fg(ACCENT_COLOR)
                            .add_modifier(Modifier::BOLD)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("Trial {} ", iteration), style),
                        Span::styled(
                            format!("({} candidates)", candidate_count),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                }
                TreeItem::Candidate { candidate_id } => {
                    let candidate = app
                        .id_to_index
                        .get(candidate_id)
                        .and_then(|&idx| app.candidates.get(idx));

                    let is_pareto = app.is_pareto(*candidate_id);

                    let (method_icon, method_color) = match candidate.map(|c| &c.method) {
                        Some(CandidateMethod::Initial) => ("◆", Color::Blue),
                        Some(CandidateMethod::Reflection) => ("◇", Color::Yellow),
                        Some(CandidateMethod::Merge) => ("◈", Color::Magenta),
                        None => ("?", Color::Gray),
                    };

                    let style = if is_selected {
                        Style::default().bg(SELECTED_BG)
                    } else {
                        Style::default()
                    };

                    // Add star for Pareto frontier candidates
                    let pareto_indicator = if is_pareto { "★ " } else { "  " };

                    // Build metrics display based on configured objectives
                    let mut spans = vec![
                        Span::styled(pareto_indicator, Style::default().fg(Color::Yellow)),
                        Span::styled(method_icon, Style::default().fg(method_color)),
                        Span::styled(
                            format!(" #{:<2}", candidate_id),
                            style.add_modifier(Modifier::BOLD),
                        ),
                    ];

                    // Add objective metrics
                    if let Some(scores) = candidate.and_then(|c| c.scores.as_ref()) {
                        for obj in &app.objectives {
                            let value = App::get_objective_value(obj, scores);
                            let (text, color) = format_compact_metric(obj, value);
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(text, Style::default().fg(color)));
                        }

                        // If no objectives configured, show default accuracy
                        if app.objectives.is_empty() {
                            let text = format!("{:.0}%", scores.test_pass_rate * 100.0);
                            let color = score_color(scores.test_pass_rate);
                            spans.push(Span::raw(" "));
                            spans.push(Span::styled(text, Style::default().fg(color)));
                        }
                    } else {
                        spans.push(Span::styled(" —", Style::default().fg(Color::Gray)));
                    }

                    ListItem::new(Line::from(spans)).style(style)
                }
            }
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(SELECTED_BG));

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// Render the right details panel
fn render_details_panel(frame: &mut Frame, app: &App, area: Rect) {
    // Calculate height needed for objectives (dynamic based on number of objectives)
    let num_objectives = app.objectives.len().max(1);
    let metadata_height = 6 + num_objectives as u16; // Base height + objectives

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(metadata_height), // Metadata (parents, scores)
            Constraint::Min(10),                 // Prompt preview
        ])
        .split(area);

    render_metadata_panel(frame, app, chunks[0]);
    render_prompt_panel(frame, app, chunks[1]);
}

/// Render candidate metadata (parents, scores)
fn render_metadata_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Candidate Details ")
        .title_style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let content = if let Some(candidate) = app.selected_candidate() {
        let method_str = match &candidate.method {
            CandidateMethod::Initial => "Initial (user's original)",
            CandidateMethod::Reflection => "Reflection (improved from failures)",
            CandidateMethod::Merge => "Merge (combined candidates)",
        };

        let parents_str = if candidate.parent_ids.is_empty() {
            "None (initial candidate)".to_string()
        } else {
            candidate
                .parent_ids
                .iter()
                .map(|id| format!("#{}", id))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let is_pareto = app.is_pareto(candidate.id);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("#{}", candidate.id),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled("Method: ", Style::default().fg(Color::Gray)),
                Span::styled(method_str, Style::default().fg(ACCENT_COLOR)),
                if is_pareto {
                    Span::styled("  ★ Pareto", Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(vec![
                Span::styled("Parent(s): ", Style::default().fg(Color::Gray)),
                Span::styled(parents_str, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
        ];

        // Add objective-specific scores
        if let Some(scores) = &candidate.scores {
            lines.push(Line::from(Span::styled(
                "Optimization Metrics:",
                Style::default()
                    .fg(HEADER_COLOR)
                    .add_modifier(Modifier::BOLD),
            )));

            if app.objectives.is_empty() {
                // Show default metrics if no objectives configured
                lines.extend(format_default_scores(scores));
            } else {
                // Show only the configured objectives
                for obj in &app.objectives {
                    let value = App::get_objective_value(obj, scores);
                    let (formatted_value, color) = format_objective_value(obj, value);

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{} ", obj.name),
                            Style::default().fg(Color::Gray),
                        ),
                        Span::styled(
                            format!("({}%, {}): ", (obj.weight * 100.0) as i32, obj.direction),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(formatted_value, Style::default().fg(color)),
                    ]));
                }
            }

            // Add check scores if any
            if !scores.check_scores.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("Checks: ", Style::default().fg(Color::Gray)),
                ]));
                for (name, rate) in &scores.check_scores {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(format!("{}: ", name), Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("{:.0}%", rate * 100.0),
                            Style::default().fg(score_color(*rate)),
                        ),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(Span::styled(
                "Not yet evaluated",
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        lines
    } else {
        vec![Line::from(Span::styled(
            "Select a candidate to view details",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
        ))]
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// Format an objective value for display
fn format_objective_value(obj: &ObjectiveConfig, value: f64) -> (String, Color) {
    match obj.name.as_str() {
        "accuracy" => {
            let formatted = format!("{:.1}%", value * 100.0);
            let color = score_color(value);
            (formatted, color)
        }
        "tokens" | "prompt_tokens" | "completion_tokens" => {
            let formatted = format!("{:.0} tokens", value);
            // Lower is better for tokens
            let color = if value < 100.0 {
                SCORE_GOOD
            } else if value < 500.0 {
                SCORE_MED
            } else {
                SCORE_BAD
            };
            (formatted, color)
        }
        "latency" => {
            let formatted = format!("{:.0}ms", value);
            // Lower is better for latency
            let color = if value < 500.0 {
                SCORE_GOOD
            } else if value < 2000.0 {
                SCORE_MED
            } else {
                SCORE_BAD
            };
            (formatted, color)
        }
        _ => {
            let formatted = format!("{:.2}", value);
            (formatted, Color::White)
        }
    }
}

/// Format default scores when no objectives are configured
fn format_default_scores(scores: &CandidateScores) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Pass Rate: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!(
                    "{:.1}% ({}/{} tests)",
                    scores.test_pass_rate * 100.0,
                    scores.tests_passed,
                    scores.tests_total
                ),
                Style::default().fg(score_color(scores.test_pass_rate)),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Tokens: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!(
                    "in: {:.0}  out: {:.0}",
                    scores.avg_prompt_tokens, scores.avg_completion_tokens
                ),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Latency: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:.0}ms", scores.avg_latency_ms),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ]
}

/// Render the prompt preview panel
fn render_prompt_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Prompt Preview ")
        .title_style(
            Style::default()
                .fg(HEADER_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let inner_area = block.inner(area);

    let content = if let Some(candidate) = app.selected_candidate() {
        // Format the prompt in BAML-like style
        let mut text = String::new();

        // Function header
        text.push_str(&format!(
            "function {} {{\n",
            candidate.function.function_name
        ));
        text.push_str("  prompt #\"\n");

        // Prompt text with indentation
        for line in candidate.function.prompt_text.lines() {
            text.push_str("    ");
            text.push_str(line);
            text.push('\n');
        }

        text.push_str("  \"#\n");
        text.push_str("}\n\n");

        // Class definitions
        for class in &candidate.function.classes {
            if let Some(desc) = &class.description {
                text.push_str(&format!("@@description(\"{}\")\n", desc));
            }
            text.push_str(&format!("class {} {{\n", class.class_name));

            for field in &class.fields {
                text.push_str(&format!("  {} {}", field.field_name, field.field_type));
                if let Some(desc) = &field.description {
                    text.push_str(&format!(" @description(\"{}\")", desc));
                }
                if let Some(alias) = &field.alias {
                    text.push_str(&format!(" @alias({})", alias));
                }
                text.push('\n');
            }

            text.push_str("}\n\n");
        }

        // Enum definitions
        for enum_def in &candidate.function.enums {
            text.push_str(&format!("enum {} {{\n", enum_def.enum_name));
            for value in &enum_def.values {
                text.push_str(&format!("  {}", value));
                if let Some(desc) = enum_def.value_descriptions.get(value) {
                    text.push_str(&format!(" // {}", desc));
                }
                text.push('\n');
            }
            text.push_str("}\n\n");
        }

        syntax_highlight(&text)
    } else {
        Text::from(Span::styled(
            "Select a candidate to view its prompt",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
        ))
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.prompt_scroll, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar
    if let Some(candidate) = app.selected_candidate() {
        let line_count = candidate.function.prompt_text.lines().count()
            + candidate
                .function
                .classes
                .iter()
                .map(|c| c.fields.len() + 3)
                .sum::<usize>()
            + candidate
                .function
                .enums
                .iter()
                .map(|e| e.values.len() + 2)
                .sum::<usize>()
            + 10;

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let mut scrollbar_state =
            ScrollbarState::new(line_count).position(app.prompt_scroll as usize);

        frame.render_stateful_widget(
            scrollbar,
            inner_area.inner(ratatui::layout::Margin {
                horizontal: 0,
                vertical: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

/// Render the footer with key hints
fn render_footer(frame: &mut Frame, area: Rect) {
    let hints = Line::from(vec![
        Span::styled(
            " ↑/↓ ",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Navigate  "),
        Span::styled(
            " [/] ",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Scroll prompt  "),
        Span::styled(
            " ★ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Pareto frontier  "),
        Span::styled(
            " q/Esc ",
            Style::default()
                .fg(ACCENT_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Quit"),
    ]);

    let paragraph = Paragraph::new(hints).style(Style::default().fg(Color::Gray));

    frame.render_widget(paragraph, area);
}

/// Simple syntax highlighting for BAML-like code
fn syntax_highlight(code: &str) -> Text<'static> {
    let mut lines = Vec::new();

    for line in code.lines() {
        let mut spans = Vec::new();
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];

        spans.push(Span::raw(indent.to_string()));

        // Highlight keywords
        if trimmed.starts_with("function ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("enum ")
        {
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            if parts.len() == 2 {
                spans.push(Span::styled(
                    parts[0].to_string(),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
                // Find the name (before {)
                let rest = parts[1];
                if let Some(brace_pos) = rest.find('{') {
                    spans.push(Span::styled(
                        rest[..brace_pos].trim().to_string(),
                        Style::default().fg(Color::Cyan),
                    ));
                    spans.push(Span::styled(" {".to_string(), Style::default().fg(Color::Gray)));
                } else {
                    spans.push(Span::raw(rest.to_string()));
                }
            } else {
                spans.push(Span::raw(trimmed.to_string()));
            }
        } else if trimmed.starts_with("prompt ")
            || trimmed.starts_with("\"#")
            || trimmed.starts_with("#\"")
        {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::Green),
            ));
        } else if trimmed.starts_with("//") {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ));
        } else if trimmed.starts_with('@') {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::Yellow),
            ));
        } else if trimmed == "}" || trimmed == "{" {
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::Gray),
            ));
        } else if trimmed.contains(" @") {
            // Field with annotations
            if let Some(at_pos) = trimmed.find(" @") {
                let (field_part, annotation_part) = trimmed.split_at(at_pos);
                // Highlight field name and type
                let field_parts: Vec<&str> = field_part.splitn(2, ' ').collect();
                if field_parts.len() == 2 {
                    spans.push(Span::styled(
                        field_parts[0].to_string(),
                        Style::default().fg(Color::White),
                    ));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        field_parts[1].to_string(),
                        Style::default().fg(Color::Cyan),
                    ));
                } else {
                    spans.push(Span::raw(field_part.to_string()));
                }
                spans.push(Span::styled(
                    annotation_part.to_string(),
                    Style::default().fg(Color::Yellow),
                ));
            } else {
                spans.push(Span::raw(trimmed.to_string()));
            }
        } else {
            // Regular text (inside prompt)
            spans.push(Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::White),
            ));
        }

        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

/// Get color based on score (0.0 to 1.0)
fn score_color(score: f64) -> Color {
    if score >= 0.8 {
        SCORE_GOOD
    } else if score >= 0.5 {
        SCORE_MED
    } else {
        SCORE_BAD
    }
}

/// Format a metric value compactly for the left panel display
/// Returns a short string and appropriate color
fn format_compact_metric(obj: &ObjectiveConfig, value: f64) -> (String, Color) {
    match obj.name.as_str() {
        "accuracy" => {
            let text = format!("{:.0}%", value * 100.0);
            let color = score_color(value);
            (text, color)
        }
        "tokens" => {
            // Combined tokens - use "t" suffix for brevity
            let text = format!("{:.0}t", value);
            // Lower is better for tokens
            let color = if value < 200.0 {
                SCORE_GOOD
            } else if value < 500.0 {
                SCORE_MED
            } else {
                SCORE_BAD
            };
            (text, color)
        }
        "prompt_tokens" => {
            let text = format!("p:{:.0}", value);
            let color = if value < 100.0 {
                SCORE_GOOD
            } else if value < 300.0 {
                SCORE_MED
            } else {
                SCORE_BAD
            };
            (text, color)
        }
        "completion_tokens" => {
            let text = format!("c:{:.0}", value);
            let color = if value < 100.0 {
                SCORE_GOOD
            } else if value < 300.0 {
                SCORE_MED
            } else {
                SCORE_BAD
            };
            (text, color)
        }
        "latency" => {
            // Format latency - use ms or s depending on magnitude
            let (text, color) = if value < 1000.0 {
                (
                    format!("{:.0}ms", value),
                    if value < 500.0 { SCORE_GOOD } else { SCORE_MED },
                )
            } else {
                (
                    format!("{:.1}s", value / 1000.0),
                    if value < 2000.0 { SCORE_MED } else { SCORE_BAD },
                )
            };
            (text, color)
        }
        name if name.starts_with("check:") => {
            // Check scores are 0.0-1.0, display as percentage
            let text = format!("{:.0}%", value * 100.0);
            let color = score_color(value);
            (text, color)
        }
        _ => {
            // Unknown metric - just display the raw value
            let text = format!("{:.1}", value);
            (text, Color::White)
        }
    }
}

/// Run the TUI application
pub fn run_tui(storage_path: &Path) -> Result<()> {
    let storage = OptimizationStorage::from_existing(storage_path)
        .context("Failed to open optimization storage")?;

    let mut app = App::from_storage(&storage)?;

    if app.candidates.is_empty() {
        anyhow::bail!("No candidates found in {}", storage_path.display());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main event loop
fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| render_ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key.code, key.modifiers);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// =============================================================================
// Post-optimization Pareto frontier display and selection
// =============================================================================

/// Display Pareto frontier candidates and let user choose one to apply
pub fn display_pareto_and_select(
    candidates: &[Candidate],
    pareto_ids: &[usize],
    objectives: &[ObjectiveConfig],
    function_name: &str,
) -> Option<usize> {
    if pareto_ids.is_empty() {
        println!("\nNo candidates on the Pareto frontier.");
        return None;
    }

    // Build a map of candidate ID to candidate
    let id_to_candidate: HashMap<usize, &Candidate> =
        candidates.iter().map(|c| (c.id, c)).collect();

    println!("\n{}", "═".repeat(70));
    println!(
        "  {} Pareto Frontier Candidates for {}",
        "★".to_string(),
        function_name
    );
    println!("{}", "═".repeat(70));

    // Print header
    let mut header = format!("  {:>4} │", "ID");
    for obj in objectives {
        header.push_str(&format!(" {:>12} │", obj.name));
    }
    println!("{}", header);
    println!("  {}", "─".repeat(66));

    // Print each Pareto candidate
    for &id in pareto_ids {
        if let Some(candidate) = id_to_candidate.get(&id) {
            if let Some(scores) = &candidate.scores {
                let mut row = format!("  #{:>3} │", id);
                for obj in objectives {
                    let value = App::get_objective_value(obj, scores);
                    let formatted = match obj.name.as_str() {
                        "accuracy" => format!("{:.1}%", value * 100.0),
                        "tokens" | "prompt_tokens" | "completion_tokens" => {
                            format!("{:.0}", value)
                        }
                        "latency" => format!("{:.0}ms", value),
                        _ => format!("{:.2}", value),
                    };
                    row.push_str(&format!(" {:>12} │", formatted));
                }
                println!("{}", row);
            }
        }
    }

    println!("{}", "═".repeat(70));
    println!();

    // If only one candidate, suggest it
    if pareto_ids.len() == 1 {
        println!(
            "Only one candidate on the Pareto frontier: #{}",
            pareto_ids[0]
        );
        print!("Apply this candidate? [Y/n]: ");
        io::Write::flush(&mut io::stdout()).ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let input = input.trim().to_lowercase();
            if input.is_empty() || input == "y" || input == "yes" {
                return Some(pareto_ids[0]);
            }
        }
        return None;
    }

    // Let user choose
    println!("Enter candidate ID to apply (or press Enter to skip):");
    print!("> ");
    io::Write::flush(&mut io::stdout()).ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        // Parse the input, removing '#' if present
        let id_str = input.trim_start_matches('#');
        if let Ok(id) = id_str.parse::<usize>() {
            if pareto_ids.contains(&id) {
                return Some(id);
            } else {
                println!("Candidate #{} is not on the Pareto frontier.", id);
            }
        } else {
            println!("Invalid input: {}", input);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimize::candidate::OptimizableFunction;

    fn make_test_candidate(id: usize, iteration: usize, pass_rate: f64) -> Candidate {
        Candidate {
            id,
            iteration,
            parent_ids: if id == 0 { vec![] } else { vec![id - 1] },
            method: if id == 0 {
                CandidateMethod::Initial
            } else {
                CandidateMethod::Reflection
            },
            function: OptimizableFunction {
                function_name: "TestFunction".to_string(),
                prompt_text: "Test prompt".to_string(),
                classes: vec![],
                enums: vec![],
                function_source: None,
            },
            scores: Some(CandidateScores {
                test_pass_rate: pass_rate,
                tests_passed: (pass_rate * 10.0) as usize,
                tests_total: 10,
                avg_prompt_tokens: 100.0,
                avg_completion_tokens: 50.0,
                avg_latency_ms: 500.0,
                check_scores: std::collections::HashMap::new(),
            }),
        }
    }

    #[test]
    fn test_app_creation() {
        let candidates = vec![
            make_test_candidate(0, 0, 0.5),
            make_test_candidate(1, 1, 0.7),
            make_test_candidate(2, 2, 0.9),
        ];

        let app = App::from_candidates_with_config(
            candidates,
            "TestFunc".to_string(),
            "/tmp/test".to_string(),
            vec![],
            vec![],
        )
        .unwrap();

        assert_eq!(app.candidates.len(), 3);
        assert_eq!(app.tree_items.len(), 6); // 3 trials + 3 candidates
    }

    #[test]
    fn test_navigation() {
        let candidates = vec![
            make_test_candidate(0, 0, 0.5),
            make_test_candidate(1, 1, 0.7),
        ];

        let mut app = App::from_candidates_with_config(
            candidates,
            "TestFunc".to_string(),
            "/tmp/test".to_string(),
            vec![],
            vec![],
        )
        .unwrap();

        assert_eq!(app.selected_index, 0);

        app.select_next();
        assert_eq!(app.selected_index, 1);

        app.select_next();
        assert_eq!(app.selected_index, 2);

        app.select_previous();
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_score_color() {
        assert_eq!(score_color(1.0), SCORE_GOOD);
        assert_eq!(score_color(0.8), SCORE_GOOD);
        assert_eq!(score_color(0.6), SCORE_MED);
        assert_eq!(score_color(0.3), SCORE_BAD);
    }

    #[test]
    fn test_pareto_detection() {
        let candidates = vec![
            make_test_candidate(0, 0, 0.5),
            make_test_candidate(1, 1, 0.7),
        ];

        let app = App::from_candidates_with_config(
            candidates,
            "TestFunc".to_string(),
            "/tmp/test".to_string(),
            vec![],
            vec![1], // Only candidate 1 is on Pareto frontier
        )
        .unwrap();

        assert!(!app.is_pareto(0));
        assert!(app.is_pareto(1));
    }
}
