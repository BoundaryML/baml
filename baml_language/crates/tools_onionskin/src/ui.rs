use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use similar::{ChangeTag, TextDiff};

use crate::{
    app::{App, RebuildState},
    compiler::{CompilerPhase, LineStatus, ThirDisplayMode, VisualizationMode},
};

pub(crate) fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub(crate) fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    // Check which banners to show
    let show_rebuild_banner = !matches!(app.rebuild_state(), RebuildState::Idle);
    let show_debug_banner = !app.debug_messages().is_empty();

    // Build constraints dynamically based on which banners are shown
    let mut constraints = vec![
        Constraint::Length(3), // Header
        Constraint::Length(3), // Phase tabs
    ];

    if show_rebuild_banner {
        constraints.push(Constraint::Length(3)); // Rebuild status banner
    }
    if show_debug_banner {
        // Dynamic height based on number of messages (min 3, max 8)
        let debug_height = (app.debug_messages().len() + 2).clamp(3, 8) as u16;
        constraints.push(Constraint::Length(debug_height)); // Debug log banner
    }

    constraints.push(Constraint::Min(0)); // Content
    constraints.push(Constraint::Length(6)); // Status bar (4 text lines + 2 border rows)

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    let mut chunk_idx = 0;

    // Draw header
    draw_header(frame, chunks[chunk_idx], app);
    chunk_idx += 1;

    // Draw phase tabs
    draw_phase_tabs(frame, chunks[chunk_idx], app);
    chunk_idx += 1;

    // Draw rebuild banner if needed
    if show_rebuild_banner {
        draw_rebuild_banner(frame, chunks[chunk_idx], app);
        chunk_idx += 1;
    }

    // Draw debug banner if needed
    if show_debug_banner {
        draw_debug_banner(frame, chunks[chunk_idx], app);
        chunk_idx += 1;
    }

    // Draw content (either single view or diff view)
    if app.has_snapshot() {
        draw_diff_view(frame, chunks[chunk_idx], app);
    } else {
        draw_single_view(frame, chunks[chunk_idx], app);
    }
    chunk_idx += 1;

    // Draw status bar
    draw_status_bar(frame, chunks[chunk_idx], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mode = if app.file_path().is_dir() {
        "Directory"
    } else {
        "File"
    };

    let hot_reload_indicator = if app.is_hot_reload_enabled() {
        " | 🔥 Hot-Reload"
    } else {
        ""
    };

    let title = format!(
        "BAML Onionskin [{}]: {}{}{}",
        mode,
        app.file_path().display(),
        if app.has_snapshot() {
            " | Snapshot: ON"
        } else {
            ""
        },
        hot_reload_indicator
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(Color::Cyan));

    frame.render_widget(block, area);
}

fn draw_phase_tabs(frame: &mut Frame, area: Rect, app: &App) {
    use crate::compiler::RecomputationStatus;

    let mut spans = Vec::new();

    for (i, phase) in CompilerPhase::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" │ "));
        }

        let is_selected = *phase == app.current_phase();

        // White by default, color only if selected
        let style = if is_selected {
            let status = app.get_recomputation_status(*phase);

            // Choose color based on recomputation status for selected tab
            let color = match status {
                RecomputationStatus::Summary {
                    recomputed_count,
                    cached_count,
                } => {
                    if recomputed_count > 0 && cached_count == 0 {
                        Color::Red // All recomputed
                    } else if recomputed_count > 0 && cached_count > 0 {
                        Color::Yellow // Mixed
                    } else {
                        Color::Green // All cached
                    }
                }
            };

            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::White)
        };

        spans.push(Span::styled(phase.name(), style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Compiler Phase"),
    );

    frame.render_widget(paragraph, area);
}

fn draw_rebuild_banner(frame: &mut Frame, area: Rect, app: &App) {
    let (message, style) = match app.rebuild_state() {
        RebuildState::Idle => ("".to_string(), Style::default()),
        RebuildState::Building => (
            "🔨 Building... (this may take a moment)".to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        RebuildState::ReadyToSwap => (
            "✓ Build successful! Press [Enter] to swap to new binary, [Esc] to dismiss".to_string(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        RebuildState::Failed(error) => (
            format!("✗ Build failed: {}  [Esc] to dismiss", error),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    };

    let paragraph = Paragraph::new(message).style(style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Compiler Hot-Reload")
            .border_style(style),
    );

    frame.render_widget(paragraph, area);
}

fn draw_debug_banner(frame: &mut Frame, area: Rect, app: &App) {
    let messages = app.debug_messages();
    let count = messages.len();

    // Build lines for display
    let mut lines: Vec<Line> = Vec::new();

    for msg in messages.iter().take(6) {
        // Extract just the crate name from the module path for brevity
        let crate_name = msg.module.split("::").next().unwrap_or(msg.module);
        let line = Line::from(vec![
            Span::styled(
                format!("[{}] ", crate_name),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(&msg.message),
        ]);
        lines.push(line);
    }

    if count > 6 {
        lines.push(Line::from(Span::styled(
            format!("... and {} more", count - 6),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let title = format!("Debug Log ({} messages) - [d] Dismiss", count);
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn draw_single_view(frame: &mut Frame, area: Rect, app: &App) {
    let phase = app.current_phase();

    // Special handling for THIR interactive mode
    if phase == CompilerPhase::Thir && app.thir_display_mode() == ThirDisplayMode::Interactive {
        draw_thir_interactive_view(frame, area, app);
        return;
    }

    // HIR2 column browser
    if phase == CompilerPhase::Hir2 && app.hir2_column_active() {
        draw_hir2_column_view(frame, area, app);
        return;
    }

    // TIR2 column browser
    if phase == CompilerPhase::Tir2 && app.tir2_column_active() {
        draw_tir2_column_view(frame, area, app);
        return;
    }

    if app.visualization_mode() == VisualizationMode::Incremental && phase == CompilerPhase::Parser
    {
        let annotated = app.get_output_annotated(phase);
        if !annotated.is_empty() {
            let paragraph = Paragraph::new(annotated_lines_to_text(
                &annotated,
                app.visualization_mode(),
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Output")
                    .style(Style::default()),
            )
            .scroll((app.scroll_offset(), 0))
            .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
            return;
        }
    }

    let output = app.current_output();
    let paragraph = Paragraph::new(output)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Output")
                .style(Style::default()),
        )
        .scroll((app.scroll_offset(), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Draw the THIR interactive view with cursor navigation
fn draw_thir_interactive_view(frame: &mut Frame, area: Rect, app: &App) {
    let state = app.thir_interactive_state();
    let cursor_line = state.cursor_line;
    let cursor_col = state.cursor_col;

    // Build styled lines with cursor highlight
    let mut lines: Vec<Line> = Vec::new();
    for (i, line_text) in state.source_lines.iter().enumerate() {
        if i == cursor_line {
            // This is the cursor line - highlight the cursor position
            let mut spans = Vec::new();
            for (j, ch) in line_text.chars().enumerate() {
                if j == cursor_col {
                    // Cursor position - highlight with inverted colors
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default().bg(Color::Yellow).fg(Color::Black),
                    ));
                } else {
                    spans.push(Span::raw(ch.to_string()));
                }
            }
            // If cursor is at end of line, show cursor there
            if cursor_col >= line_text.len() {
                spans.push(Span::styled(
                    " ",
                    Style::default().bg(Color::Yellow).fg(Color::Black),
                ));
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(line_text.as_str()));
        }
    }

    let border_style = if app.thir_interactive_active() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("THIR (Interactive - hjkl/arrows to move, Esc to exit)")
                .border_style(border_style),
        )
        .scroll((app.scroll_offset(), 0));

    frame.render_widget(paragraph, area);
}

/// Draw the HIR2 column browser (macOS Finder-style Miller columns).
fn draw_tir2_column_view(frame: &mut Frame, area: Rect, app: &App) {
    let data = app.tir2_column_data();
    let state = app.tir2_column_state();
    let view = ColumnView::from_tir2(data, state);
    draw_column_browser(frame, area, "TIR2", &view);
}

fn draw_hir2_column_view(frame: &mut Frame, area: Rect, app: &App) {
    let data = app.hir2_column_data();
    let state = app.hir2_column_state();
    let view = ColumnView::from_hir2(data, state);
    draw_column_browser(frame, area, "HIR2", &view);
}

// ── Generic Column Browser ──────────────────────────────────────────────────

/// Type-erased column browser view. Built from either HIR2 or TIR2 data.
struct ColumnView<'a> {
    packages: Vec<ColPkgView<'a>>,
    active_column: usize,
    selected: [usize; 3],
    detail_scroll: usize,
}

struct ColPkgView<'a> {
    name: &'a str,
    namespace: &'a str,
    files: Vec<ColFileView<'a>>,
    namespace_summary: &'a [String],
}

struct ColFileView<'a> {
    name: &'a str,
    summary: &'a str,
    items: Vec<ColItemView<'a>>,
    detail_lines: &'a [String],
    error_count: usize,
}

struct ColItemView<'a> {
    name: &'a str,
    kind: &'a str,
    signature: &'a str,
    detail_lines: Vec<crate::compiler::DetailLine>,
    has_errors: bool,
}

impl<'a> ColumnView<'a> {
    fn from_hir2(
        data: &'a crate::compiler::Hir2ColumnData,
        state: &'a crate::compiler::Hir2ColumnState,
    ) -> Self {
        Self {
            packages: data
                .packages
                .iter()
                .map(|p| ColPkgView {
                    name: &p.name,
                    namespace: &p.namespace,
                    namespace_summary: &p.namespace_summary,
                    files: p
                        .files
                        .iter()
                        .map(|f| ColFileView {
                            name: &f.name,
                            summary: &f.summary,
                            detail_lines: &f.detail_lines,
                            error_count: f.error_count,
                            items: f
                                .items
                                .iter()
                                .map(|i| ColItemView {
                                    name: &i.name,
                                    kind: &i.kind,
                                    signature: &i.signature,
                                    detail_lines: i
                                        .detail_lines
                                        .iter()
                                        .map(|s| crate::compiler::plain(s.clone()))
                                        .collect(),
                                    has_errors: i.has_errors,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            active_column: state.active_column,
            selected: state.selected,
            detail_scroll: state.detail_scroll,
        }
    }

    fn from_tir2(
        data: &'a crate::compiler::Tir2ColumnData,
        state: &'a crate::compiler::Tir2ColumnState,
    ) -> Self {
        Self {
            packages: data
                .packages
                .iter()
                .map(|p| ColPkgView {
                    name: &p.name,
                    namespace: &p.namespace,
                    namespace_summary: &p.namespace_summary,
                    files: p
                        .files
                        .iter()
                        .map(|f| ColFileView {
                            name: &f.name,
                            summary: &f.summary,
                            detail_lines: &f.detail_lines,
                            error_count: f.error_count,
                            items: f
                                .items
                                .iter()
                                .map(|i| ColItemView {
                                    name: &i.name,
                                    kind: &i.kind,
                                    signature: &i.signature,
                                    detail_lines: i.detail_lines.clone(),
                                    has_errors: i.has_errors,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            active_column: state.active_column,
            selected: state.selected,
            detail_scroll: state.detail_scroll,
        }
    }
}

fn draw_column_browser(frame: &mut Frame, area: Rect, phase_label: &str, view: &ColumnView) {
    if view.packages.is_empty() {
        let paragraph = Paragraph::new("No packages found").block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{phase_label} Browser")),
        );
        frame.render_widget(paragraph, area);
        return;
    }

    // Layout: 3 list columns + 1 detail pane
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22), // packages
            Constraint::Length(28), // files
            Constraint::Length(30), // items
            Constraint::Min(20),    // detail
        ])
        .split(area);

    let selected_fg = Color::Black;
    let selected_bg = Color::Cyan;
    let active_border = Style::default().fg(Color::Yellow);
    let normal_border = Style::default().fg(Color::DarkGray);

    // ── Column 0: Packages ──────────────────────────────────────
    {
        let mut lines: Vec<Line> = Vec::new();
        for (i, pkg) in view.packages.iter().enumerate() {
            let is_selected = i == view.selected[0];
            let has_errors = pkg.files.iter().any(|f| f.error_count > 0);
            let label = format!("{} {}", pkg.name, pkg.namespace);
            let style = if is_selected {
                Style::default()
                    .fg(selected_fg)
                    .bg(selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else if has_errors {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(label, style)];
            if has_errors && !is_selected {
                let total: usize = pkg.files.iter().map(|f| f.error_count).sum();
                spans.push(Span::styled(
                    format!(" ({total})"),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(spans));
        }
        let border = if view.active_column == 0 {
            active_border
        } else {
            normal_border
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Package")
            .border_style(border);
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, chunks[0]);
    }

    // ── Column 1: Files ─────────────────────────────────────────
    let pkg = &view.packages[view.selected[0]];
    {
        let mut lines: Vec<Line> = Vec::new();
        for (i, file) in pkg.files.iter().enumerate() {
            let is_selected = i == view.selected[1];
            let has_errors = file.error_count > 0;
            let name_style = if is_selected {
                Style::default()
                    .fg(selected_fg)
                    .bg(selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else if has_errors {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(file.name.to_string(), name_style)];
            if has_errors {
                let err_style = if is_selected {
                    Style::default()
                        .fg(Color::Red)
                        .bg(selected_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                };
                spans.push(Span::styled(format!(" ({})", file.error_count), err_style));
            }
            lines.push(Line::from(spans));
            if is_selected {
                lines.push(Line::from(Span::styled(
                    format!("  {}", file.summary),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
        let border = if view.active_column == 1 {
            active_border
        } else {
            normal_border
        };
        let title = format!("Files ({})", pkg.files.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border);
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, chunks[1]);
    }

    // ── Column 2: Items ─────────────────────────────────────────
    let file = if !pkg.files.is_empty() {
        Some(&pkg.files[view.selected[1]])
    } else {
        None
    };
    {
        let mut lines: Vec<Line> = Vec::new();
        if let Some(f) = file {
            for (i, item) in f.items.iter().enumerate() {
                let is_selected = i == view.selected[2];
                let kind_style = if is_selected {
                    Style::default().fg(selected_fg).bg(selected_bg)
                } else if item.has_errors {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::Green)
                };
                let name_style = if is_selected {
                    Style::default()
                        .fg(selected_fg)
                        .bg(selected_bg)
                        .add_modifier(Modifier::BOLD)
                } else if item.has_errors {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let sig_style = if is_selected {
                    Style::default().fg(selected_fg).bg(selected_bg)
                } else if item.has_errors {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let mut spans = vec![
                    Span::styled(format!("{:<5} ", item.kind), kind_style),
                    Span::styled(item.name.to_string(), name_style),
                ];
                if !item.signature.is_empty() {
                    spans.push(Span::styled(format!(" {}", item.signature), sig_style));
                }
                lines.push(Line::from(spans));
            }
        }
        let border = if view.active_column == 2 {
            active_border
        } else {
            normal_border
        };
        let item_count = file.map(|f| f.items.len()).unwrap_or(0);
        let title = format!("Items ({})", item_count);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border);
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, chunks[2]);
    }

    // ── Column 3: Detail (scrollable) ───────────────────────────
    {
        use crate::compiler::DetailSpan;

        let rich_lines: Vec<crate::compiler::DetailLine> = match view.active_column {
            0 => pkg
                .namespace_summary
                .iter()
                .map(|s| crate::compiler::plain(s.clone()))
                .collect(),
            1 => file
                .map(|f| {
                    f.detail_lines
                        .iter()
                        .map(|s| crate::compiler::plain(s.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            _ => file
                .and_then(|f| f.items.get(view.selected[2]))
                .map(|item| item.detail_lines.clone())
                .unwrap_or_default(),
        };

        let total = rich_lines.len();
        let visible_height = chunks[3].height.saturating_sub(2) as usize;
        let scroll = view.detail_scroll.min(total.saturating_sub(1));

        let title = if total > visible_height {
            format!(
                "Detail [{}-{}/{}]",
                scroll + 1,
                (scroll + visible_height).min(total),
                total
            )
        } else {
            "Detail".to_string()
        };

        let lines: Vec<Line> = rich_lines
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|detail_line| {
                let spans: Vec<Span> = detail_line
                    .iter()
                    .map(|span| match span {
                        DetailSpan::Code(s) => Span::raw(s.clone()),
                        DetailSpan::TypeAnnotation(s) => {
                            Span::styled(s.clone(), Style::default().fg(Color::DarkGray))
                        }
                        DetailSpan::Error(s) => {
                            Span::styled(s.clone(), Style::default().fg(Color::Red))
                        }
                    })
                    .collect();
                Line::from(spans)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::DarkGray));
        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, chunks[3]);
    }
}

fn draw_diff_view(frame: &mut Frame, area: Rect, app: &App) {
    let phase = app.current_phase();
    let mode = app.visualization_mode();

    if mode == VisualizationMode::Incremental {
        let current_lines = app.get_output_annotated(phase);
        if let Some(snapshot_lines) = app.get_snapshot_output_annotated(phase) {
            // Split area
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let snapshot_paragraph = Paragraph::new(annotated_lines_to_text(&snapshot_lines, mode))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Snapshot")
                        .style(Style::default()),
                )
                .scroll((app.scroll_offset(), 0))
                .wrap(Wrap { trim: false });
            frame.render_widget(snapshot_paragraph, chunks[0]);

            let current_paragraph = Paragraph::new(annotated_lines_to_text(&current_lines, mode))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Current")
                        .style(Style::default()),
                )
                .scroll((app.scroll_offset(), 0))
                .wrap(Wrap { trim: false });
            frame.render_widget(current_paragraph, chunks[1]);
            return;
        }
    }

    let Some(snapshot_output) = app.snapshot_output() else {
        // Fallback to single view if no snapshot
        draw_single_view(frame, area, app);
        return;
    };

    // Split area into two columns
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Create diff
    let diff = TextDiff::from_lines(snapshot_output, app.current_output());

    // Build snapshot view
    let mut snapshot_lines = Vec::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Equal => " ",
            ChangeTag::Insert => continue, // Skip insertions in snapshot view
        };

        let style = match change.tag() {
            ChangeTag::Delete => Style::default().fg(Color::Red),
            ChangeTag::Equal => Style::default(),
            ChangeTag::Insert => Style::default(),
        };

        let line = format!("{} {}", sign, change.value().trim_end());
        snapshot_lines.push(Line::from(Span::styled(line, style)));
    }

    // Build current view
    let mut current_lines = Vec::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
            ChangeTag::Delete => continue, // Skip deletions in current view
        };

        let style = match change.tag() {
            ChangeTag::Insert => Style::default().fg(Color::Green),
            ChangeTag::Equal => Style::default(),
            ChangeTag::Delete => Style::default(),
        };

        let line = format!("{} {}", sign, change.value().trim_end());
        current_lines.push(Line::from(Span::styled(line, style)));
    }

    // Render snapshot view with synchronized scroll
    let snapshot_paragraph = Paragraph::new(Text::from(snapshot_lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Snapshot")
                .style(Style::default()),
        )
        .scroll((app.scroll_offset(), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(snapshot_paragraph, chunks[0]);

    // Render current view with synchronized scroll
    let current_paragraph = Paragraph::new(Text::from(current_lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Current")
                .style(Style::default()),
        )
        .scroll((app.scroll_offset(), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(current_paragraph, chunks[1]);
}

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let snapshot_help = if app.has_snapshot() {
        "[s] Update  [Shift+S] Delete"
    } else {
        "[s] Create"
    };

    // Build mode string, including THIR mode when on THIR phase
    let mode_str = if app.current_phase() == CompilerPhase::Thir {
        format!(
            "[m] Mode: {}  |  [t] THIR: {}",
            app.visualization_mode_name(),
            app.thir_display_mode().name()
        )
    } else {
        format!("[m] Mode: {}", app.visualization_mode_name())
    };

    // Add hot-reload rebuild shortcut if enabled
    let rebuild_str = if app.is_hot_reload_enabled() {
        "  |  [Shift+R] Rebuild"
    } else {
        ""
    };

    // Show context-specific navigation help
    let in_column_browser = (app.current_phase() == CompilerPhase::Hir2
        && app.hir2_column_active())
        || (app.current_phase() == CompilerPhase::Tir2 && app.tir2_column_active());
    let line2 = if in_column_browser {
        "[jk/↑↓] Select  [hl/←→] Column  [PgUp/PgDn/Wheel] Scroll detail  [t] Text view  [Esc] Exit  |  [q] Quit"
    } else if app.current_phase() == CompilerPhase::Thir
        && app.thir_display_mode() == ThirDisplayMode::Interactive
    {
        if app.thir_interactive_active() {
            "Navigate: [hjkl/arrows] Cursor  [Esc] Exit cursor mode  [PgUp/PgDn] Page  |  [q/Ctrl+C] Quit"
        } else {
            "Navigate: [←→] Phases  [↑↓] Scroll  [t] Activate cursor  [PgUp/PgDn] Page  |  [q/Ctrl+C] Quit"
        }
    } else if app.current_phase() == CompilerPhase::Hir2
        || app.current_phase() == CompilerPhase::Tir2
    {
        "Navigate: [←→] Phases  [↑↓] Scroll  [t] Column browser  |  [q/Ctrl+C] Quit"
    } else {
        "Navigate: [←→] Phases  [↑↓] Scroll  [PgUp/PgDn] Page  [Home] Top  [Wheel] Mouse  |  [q/Ctrl+C] Quit"
    };

    let line3_parts = vec![
        Span::raw("Phase Colors: "),
        Span::styled(
            "Red",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw("=Recomputed  "),
        Span::styled(
            "Yellow",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("=Partial  "),
        Span::styled(
            "Green",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("=Cached  "),
        Span::styled("Gray", Style::default().fg(Color::Gray)),
        Span::raw("=Headers"),
    ];

    // Build line1 with colored clipboard feedback
    let line1_parts = if let Some(status) = app.clipboard_status() {
        let is_error = status.starts_with("Failed") || status.starts_with("Clipboard unavailable");
        let status_color = if is_error { Color::Red } else { Color::Green };
        vec![
            Span::raw(format!(
                "Snapshot: {}  |  [r] Recompile  |  {}{}  |  [c/y] Copy  |  ",
                snapshot_help, mode_str, rebuild_str
            )),
            Span::styled(
                status,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![Span::raw(format!(
            "Snapshot: {}  |  [r] Recompile  |  {}{}  |  [c/y] Copy",
            snapshot_help, mode_str, rebuild_str
        ))]
    };

    let watcher_line = Line::from(Span::styled(
        app.watcher_diagnostic_summary(),
        Style::default().fg(Color::DarkGray),
    ));

    let text = vec![
        Line::from(line1_parts),
        Line::from(line2.to_string()),
        Line::from(line3_parts),
        watcher_line,
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Keyboard Shortcuts"),
        )
        .style(Style::default().fg(Color::Gray));

    frame.render_widget(paragraph, area);
}

fn annotated_lines_to_text(lines: &[(String, LineStatus)], mode: VisualizationMode) -> Text<'_> {
    let styled_lines: Vec<Line> = lines
        .iter()
        .map(|(text, status)| {
            Line::from(Span::styled(text.clone(), style_for_status(*status, mode)))
        })
        .collect();
    Text::from(styled_lines)
}

fn style_for_status(status: LineStatus, mode: VisualizationMode) -> Style {
    match mode {
        VisualizationMode::Incremental => match status {
            LineStatus::Recomputed => Style::default().fg(Color::Yellow),
            LineStatus::Cached => Style::default().fg(Color::Blue),
            LineStatus::Unknown => Style::default().fg(Color::DarkGray),
        },
        VisualizationMode::Diff => match status {
            LineStatus::Recomputed => Style::default().fg(Color::Red),
            LineStatus::Cached => Style::default().fg(Color::Green),
            LineStatus::Unknown => Style::default().fg(Color::DarkGray),
        },
    }
}
