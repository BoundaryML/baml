use std::{
    io::{self, IsTerminal},
    time::{Duration, Instant},
};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Terminal,
};

#[derive(Clone)]
pub struct InitStep {
    pub message: String,
    pub status: StepStatus,
    pub completion_time: Option<Instant>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

pub struct InitUI {
    steps: Vec<InitStep>,
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    animation_state: usize,
    dot_animation_state: usize,
    last_update: Instant,
}

const PURPLE_COLOR: Color = Color::Rgb(142, 36, 170);
const ANIMATION_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DOT_ANIMATION: &[&str] = &["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"];

impl InitUI {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;

        // Try to create the UI, but clean up raw mode if anything fails
        let result = (|| {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(stdout);
            let terminal = Terminal::new(backend)?;

            Ok(Self {
                steps: Vec::new(),
                terminal,
                animation_state: 0,
                dot_animation_state: 0,
                last_update: Instant::now(),
            })
        })();

        // If anything failed, disable raw mode before returning the error
        if result.is_err() {
            let _ = disable_raw_mode();
        }

        result
    }

    pub fn add_step(&mut self, message: String) {
        self.steps.push(InitStep {
            message,
            status: StepStatus::Pending,
            completion_time: None,
        });
    }

    pub fn update_step(&mut self, index: usize, status: StepStatus) {
        if let Some(step) = self.steps.get_mut(index) {
            step.status = status.clone();
            if status == StepStatus::Completed {
                step.completion_time = Some(Instant::now());
            }
        }
    }

    pub fn render(&mut self) -> Result<()> {
        // Update animation state - make it faster for better visual effect
        if self.last_update.elapsed() > Duration::from_millis(50) {
            self.animation_state = (self.animation_state + 1) % ANIMATION_FRAMES.len();
            self.dot_animation_state = (self.dot_animation_state + 1) % DOT_ANIMATION.len();
            self.last_update = Instant::now();
        }

        self.terminal.draw(|f| {
            let area = f.area();

            // Create a list of all lines to display
            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "🚀 Setting up BAML for you!",
                        Style::default()
                            .fg(PURPLE_COLOR)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
            ];

            // Add each step as a line
            for step in &self.steps {
                // Left-side dot animation - only show for in-progress steps
                let dot_anim = match &step.status {
                    StepStatus::InProgress => DOT_ANIMATION[self.dot_animation_state],
                    StepStatus::Pending => " ",
                    _ => "", // No space for completed/failed steps
                };

                let (icon, icon_color) = match &step.status {
                    StepStatus::Pending => (" ", Color::Gray),
                    StepStatus::InProgress => {
                        (ANIMATION_FRAMES[self.animation_state], PURPLE_COLOR)
                    }
                    StepStatus::Completed => {
                        // Add transition animation for recently completed steps
                        if let Some(completion_time) = step.completion_time {
                            let elapsed = completion_time.elapsed();
                            if elapsed < Duration::from_millis(300) {
                                ("✓", PURPLE_COLOR)
                            } else {
                                ("✓", Color::Green)
                            }
                        } else {
                            ("✓", Color::Green)
                        }
                    }
                    StepStatus::Failed => ("✗", Color::Red),
                };

                let text_color = match &step.status {
                    StepStatus::Completed => {
                        // Fade from purple to green for recently completed steps
                        if let Some(completion_time) = step.completion_time {
                            let elapsed = completion_time.elapsed();
                            if elapsed < Duration::from_millis(300) {
                                PURPLE_COLOR
                            } else {
                                Color::Green
                            }
                        } else {
                            Color::Green
                        }
                    }
                    StepStatus::Failed => Color::Red,
                    StepStatus::InProgress => Color::White,
                    StepStatus::Pending => Color::Gray,
                };

                // Check if this is a completion message (starts with emoji)
                let is_completion_message =
                    step.message.starts_with('✨') || step.message.starts_with('📚');

                let step_text = if is_completion_message {
                    // Style completion messages differently
                    Line::from(vec![
                        Span::raw("     "), // Extra indent for completion messages
                        Span::styled(
                            &step.message,
                            Style::default()
                                .fg(PURPLE_COLOR)
                                .add_modifier(Modifier::ITALIC | Modifier::BOLD),
                        ),
                    ])
                } else {
                    // Regular step styling with dot animation
                    if dot_anim.is_empty() {
                        // Completed/failed steps - no dot animation
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                format!("{icon} "),
                                Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(&step.message, Style::default().fg(text_color)),
                        ])
                    } else {
                        // Pending/in-progress steps - show dot animation
                        Line::from(vec![
                            Span::styled(
                                format!(" {dot_anim} "),
                                Style::default().fg(PURPLE_COLOR),
                            ),
                            Span::styled(
                                format!("{icon} "),
                                Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(&step.message, Style::default().fg(text_color)),
                        ])
                    }
                };

                lines.push(step_text);
            }

            // Render all lines at once
            let paragraph = Paragraph::new(lines);
            f.render_widget(paragraph, area);
        })?;

        Ok(())
    }

    pub fn cleanup(mut self) -> Result<()> {
        // Final render to ensure last state is shown
        let _ = self.render();

        // Disable raw mode but keep the content on screen
        disable_raw_mode()?;

        // Move cursor to bottom of the UI
        let height = self.steps.len() as u16 + 4; // Steps + title + spacing
        execute!(io::stdout(), crossterm::cursor::MoveDown(height))?;

        execute!(io::stdout(), LeaveAlternateScreen)?;

        Ok(())
    }
}

pub struct InitUIContext {
    ui: Option<InitUI>,
    current_step: usize,
}

impl InitUIContext {
    pub fn new(use_ui: bool) -> Result<Self> {
        let ui = if use_ui {
            // Check if stdout is a TTY before attempting to create UI
            if io::stdout().is_terminal() {
                // Try to create the UI, but gracefully fallback if it fails
                InitUI::new().ok()
            } else {
                // Not a TTY, use non-interactive mode
                None
            }
        } else {
            None
        };
        Ok(Self {
            ui,
            current_step: 0,
        })
    }

    #[allow(clippy::print_stdout)]
    pub fn add_step(&mut self, message: &str) {
        if let Some(ui) = &mut self.ui {
            ui.add_step(message.to_string());
            let _ = ui.render();
        } else {
            // Non-interactive mode: just print the step
            println!("  {}", message);
        }
    }

    pub fn set_step_status(&mut self, index: usize, status: StepStatus) {
        if let Some(ui) = &mut self.ui {
            ui.update_step(index, status);
            let _ = ui.render();
        }
    }

    pub fn render_current(&mut self) {
        if let Some(ui) = &mut self.ui {
            let _ = ui.render();
        }
    }

    #[allow(clippy::print_stdout)]
    pub fn complete_step(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.update_step(self.current_step, StepStatus::Completed);
            let _ = ui.render();
            // Add a small delay to show the completion animation
            std::thread::sleep(Duration::from_millis(300));
        } else {
            // Non-interactive mode: print completion
            println!("  ✓ Done");
        }
        self.current_step += 1;
    }

    #[allow(clippy::print_stderr)]
    pub fn fail_step(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.update_step(self.current_step, StepStatus::Failed);
            let _ = ui.render();
        } else {
            // Non-interactive mode: print failure to stderr
            eprintln!("  ✗ Failed");
        }
        self.current_step += 1;
    }

    #[allow(clippy::print_stdout)]
    pub fn add_completion_message(&mut self, message: &str) {
        if let Some(ui) = &mut self.ui {
            ui.add_step(message.to_string());
            ui.update_step(ui.steps.len() - 1, StepStatus::Completed);
            let _ = ui.render();
        } else {
            // Non-interactive mode: just print the message
            println!("\n{}", message);
        }
    }

    #[allow(clippy::print_stdout)]
    pub fn finish(self) -> Result<()> {
        if let Some(ui) = self.ui {
            // Show final state for a moment
            std::thread::sleep(Duration::from_millis(1000));
            ui.cleanup()?;
            // Add a newline after the UI
            println!();
        }
        Ok(())
    }
}

#[allow(clippy::print_stderr)]
pub fn show_error(message: &str) -> Result<()> {
    // Check if we're in a TTY before attempting to create a fancy error UI
    if !io::stdout().is_terminal() {
        // Non-interactive mode: just print the error to stderr
        eprintln!("Error: {}", message);
        return Ok(());
    }

    // Try to create the fancy error UI, but fallback gracefully if it fails
    match show_error_ui(message) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Failed to create UI, fallback to simple error message
            eprintln!("Error: {}", message);
            Ok(())
        }
    }
}

fn show_error_ui(message: &str) -> Result<()> {
    enable_raw_mode()?;

    // Ensure cleanup happens regardless of success or failure
    let result = (|| {
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|f| {
            let area = f.area();

            // Calculate popup size
            let popup_width = message.len().min(60) as u16 + 4;
            let popup_height = 7;

            let popup_area = Rect {
                x: (area.width.saturating_sub(popup_width)) / 2,
                y: (area.height.saturating_sub(popup_height)) / 2,
                width: popup_width,
                height: popup_height,
            };

            // Clear the area first
            f.render_widget(Clear, popup_area);

            // Error box
            let error_block = Block::default()
                .title(" ⚠️  Error ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(Color::Black));

            let inner = error_block.inner(popup_area);
            f.render_widget(error_block, popup_area);

            // Error message
            let error_text = vec![
                Line::from(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        message,
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "Press any key to exit",
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]),
            ];

            let paragraph = Paragraph::new(error_text)
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: true });

            f.render_widget(paragraph, inner);
        })?;

        // Wait for user input
        loop {
            if let Event::Key(_) = event::read()? {
                break;
            }
        }

        Ok(())
    })();

    // Always clean up terminal state
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);

    result
}
