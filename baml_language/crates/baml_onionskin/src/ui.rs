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
    app::App,
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Phase tabs
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status bar with shortcuts
        ])
        .split(frame.area());

    // Draw header
    draw_header(frame, chunks[0], app);

    // Draw phase tabs
    draw_phase_tabs(frame, chunks[1], app);

    // Draw content (either single view or diff view)
    if app.has_snapshot() {
        draw_diff_view(frame, chunks[2], app);
    } else {
        draw_single_view(frame, chunks[2], app);
    }

    // Draw status bar
    draw_status_bar(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mode = if app.file_path().is_dir() {
        "Directory"
    } else {
        "File"
    };

    let title = format!(
        "BAML Onionskin [{}]: {}{}",
        mode,
        app.file_path().display(),
        if app.has_snapshot() {
            " | Snapshot: ON"
        } else {
            ""
        }
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

fn draw_single_view(frame: &mut Frame, area: Rect, app: &App) {
    let phase = app.current_phase();

    // Special handling for THIR interactive mode
    if phase == CompilerPhase::Thir && app.thir_display_mode() == ThirDisplayMode::Interactive {
        draw_thir_interactive_view(frame, area, app);
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
        "[s] Update  [S] Delete"
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

    let line1 = format!(
        "Snapshot: {}  |  [r] Recompile  |  {}  |  [Tab] Next File",
        snapshot_help, mode_str
    );

    // Show THIR-specific navigation help when in interactive mode
    let line2 = if app.current_phase() == CompilerPhase::Thir
        && app.thir_display_mode() == ThirDisplayMode::Interactive
    {
        if app.thir_interactive_active() {
            "Navigate: [hjkl/arrows] Cursor  [Esc] Exit cursor mode  [PgUp/PgDn] Page  |  [q/Ctrl+C] Quit"
        } else {
            "Navigate: [←→] Phases  [↑↓] Scroll  [t] Activate cursor  [PgUp/PgDn] Page  |  [q/Ctrl+C] Quit"
        }
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

    let text = vec![
        Line::from(line1),
        Line::from(line2.to_string()),
        Line::from(line3_parts),
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
