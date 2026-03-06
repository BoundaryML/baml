use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    sync::mpsc::{Receiver, channel},
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use arboard::Clipboard;
use baml_base::DebugMessage;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    compiler::{
        CompilerPhase, CompilerRunner, GreenElementId, ThirDisplayMode, VisualizationMode,
        read_files_from_disk,
    },
    ui,
    watcher::{ChangeKind, FileWatcher},
};

/// Result of a background build
pub(crate) enum BuildResult {
    Success,
    Failed(String),
}

/// Duration to show the "Copied!" message
const COPY_FEEDBACK_DURATION: Duration = Duration::from_secs(2);

/// Duration to show rebuild status
const REBUILD_STATUS_DURATION: Duration = Duration::from_secs(5);

/// State of compiler rebuild process
#[derive(Debug, Clone)]
pub(crate) enum RebuildState {
    /// No rebuild in progress
    Idle,
    /// Currently rebuilding (auto-triggered on source change)
    Building,
    /// Build succeeded, waiting for user to press Enter to swap to new binary
    ReadyToSwap,
    /// Rebuild failed with error
    Failed(String),
}

pub(crate) struct App {
    file_path: PathBuf,
    /// Workspace root for compiler hot-reload (if enabled)
    workspace_root: Option<PathBuf>,
    /// Which tab is shown in the TUI.
    current_phase: CompilerPhase,
    /// Current files from disk (fake filesystem).
    current_files: HashMap<PathBuf, String>,
    /// Snapshot files - represents the "before" state.
    snapshot_files: Option<HashMap<PathBuf, String>>,
    /// Snapshot compiler (separate instance for the snapshot panel)
    snapshot_compiler: Option<CompilerRunner>,
    snapshot_parser_cache: Option<HashMap<PathBuf, HashSet<GreenElementId>>>,
    compiler: CompilerRunner,
    watcher: FileWatcher,
    should_quit: bool,
    scroll_offset: u16,
    /// Visualization mode: Diff or Incremental
    visualization_mode: VisualizationMode,
    last_compiled_files: HashMap<PathBuf, String>,
    /// Whether we are in THIR interactive sub-mode (cursor navigation active)
    thir_interactive_active: bool,
    /// Whether the HIR2 column browser is active (default: true when on HIR2 tab)
    hir2_column_active: bool,
    /// Whether the TIR2 column browser is active (default: true when on TIR2 tab)
    tir2_column_active: bool,
    /// Timestamp when content was last copied to clipboard (for visual feedback)
    last_copy_time: Option<Instant>,
    /// Error message from last clipboard operation
    clipboard_error: Option<String>,
    /// Current rebuild state
    rebuild_state: RebuildState,
    /// Timestamp when rebuild state was last updated
    rebuild_state_time: Option<Instant>,
    /// Receiver for background build results
    build_result_rx: Option<Receiver<BuildResult>>,
    /// Debug messages collected from the compiler (via baml_debug! macro)
    debug_messages: Vec<DebugMessage>,
}

impl App {
    pub(crate) fn new(
        path: PathBuf,
        workspace_root: Option<PathBuf>,
        initial_phase: Option<CompilerPhase>,
    ) -> Result<Self> {
        // Create watcher - with or without compiler watching
        let watcher = if let Some(ref workspace) = workspace_root {
            FileWatcher::new_with_compiler_watch(&path, workspace)?
        } else {
            FileWatcher::new(&path)?
        };

        let mut compiler = CompilerRunner::new(&path);

        // Read initial files from disk
        let current_files = read_files_from_disk(&path)?;
        let initial_files = current_files.clone();

        // Initial compilation (no snapshot)
        compiler.compile_from_filesystem(&current_files, None);

        // Drain any debug messages from initial compilation
        let debug_messages = baml_base::drain_debug_log();

        Ok(Self {
            file_path: path,
            workspace_root,
            current_phase: initial_phase.unwrap_or(CompilerPhase::Lexer),
            current_files,
            snapshot_files: None,
            snapshot_compiler: None,
            snapshot_parser_cache: None,
            compiler,
            watcher,
            should_quit: false,
            scroll_offset: 0,
            visualization_mode: VisualizationMode::Diff, // Start in Diff mode
            last_compiled_files: initial_files,
            thir_interactive_active: false,
            hir2_column_active: true,
            tir2_column_active: true,
            last_copy_time: None,
            clipboard_error: None,
            rebuild_state: RebuildState::Idle,
            rebuild_state_time: None,
            build_result_rx: None,
            debug_messages,
        })
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        while !self.should_quit {
            // Check for file changes
            if let Some(change_kind) = self.watcher.check_for_changes() {
                match change_kind {
                    ChangeKind::BamlFile => {
                        self.reload_file()?;
                    }
                    ChangeKind::CompilerSource => {
                        self.trigger_rebuild();
                    }
                }
            }

            // Check for background build completion
            if let Some(ref rx) = self.build_result_rx
                && let Ok(result) = rx.try_recv()
            {
                match result {
                    BuildResult::Success => {
                        self.rebuild_state = RebuildState::ReadyToSwap;
                        self.rebuild_state_time = Some(Instant::now());
                        self.build_result_rx = None;
                    }
                    BuildResult::Failed(error) => {
                        self.rebuild_state = RebuildState::Failed(error);
                        self.rebuild_state_time = Some(Instant::now());
                        self.build_result_rx = None;
                    }
                }
            }

            // Draw UI
            terminal.draw(|frame| ui::draw(frame, self))?;

            // Handle input with timeout
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key_event(key),
                    Event::Mouse(mouse) => self.handle_mouse_event(mouse),
                    _ => {}
                }
            }

            // Clear old rebuild status messages
            if let Some(time) = self.rebuild_state_time
                && time.elapsed() > REBUILD_STATUS_DURATION
                && matches!(self.rebuild_state, RebuildState::Failed(_))
            {
                // Keep failed state visible longer, but clear after 10 seconds
                if time.elapsed() > Duration::from_secs(10) {
                    self.rebuild_state = RebuildState::Idle;
                    self.rebuild_state_time = None;
                }
            }
        }

        Ok(())
    }

    fn reload_file(&mut self) -> Result<()> {
        // Read current files from disk into fake filesystem
        self.current_files = read_files_from_disk(&self.file_path)?;

        self.compile_current_state();
        Ok(())
    }

    fn recompile(&mut self) {
        self.compile_current_state();
    }

    /// Trigger a compiler rebuild and restart (runs in background thread)
    fn trigger_rebuild(&mut self) {
        // Don't start another build if one is already running
        if self.build_result_rx.is_some() {
            return;
        }

        self.rebuild_state = RebuildState::Building;
        self.rebuild_state_time = Some(Instant::now());

        // Create channel for build result
        let (tx, rx) = channel();
        self.build_result_rx = Some(rx);

        // Clone workspace root for the thread
        let workspace_dir = self
            .workspace_root
            .clone()
            .unwrap_or_else(|| self.file_path.clone());

        // Spawn background thread to run cargo build
        thread::spawn(move || {
            let result = Command::new("cargo")
                .args(["build", "--bin", "tools_onionskin"])
                .current_dir(&workspace_dir)
                .output();

            let build_result = match result {
                Ok(output) => {
                    if output.status.success() {
                        BuildResult::Success
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        // Extract just the first few lines of error
                        let error_summary: String = stderr
                            .lines()
                            .filter(|line| line.contains("error"))
                            .take(3)
                            .collect::<Vec<_>>()
                            .join("\n");

                        BuildResult::Failed(if error_summary.is_empty() {
                            "Build failed (see terminal)".to_string()
                        } else {
                            error_summary
                        })
                    }
                }
                Err(e) => BuildResult::Failed(format!("Failed to run cargo: {}", e)),
            };

            // Send result back (ignore error if receiver dropped)
            let _ = tx.send(build_result);
        });
    }

    /// Restart by exec'ing into the new binary
    fn exec_restart(&self) -> ! {
        // Restore terminal before exec
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );

        // Get current exe path
        let exe = std::env::current_exe().expect("Failed to get current executable path");

        // Collect current args, filtering out any existing --phase argument
        let mut args: Vec<String> = std::env::args()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .fold((Vec::new(), false), |(mut acc, skip_next), arg| {
                if skip_next {
                    // Skip this arg (it's the value after --phase)
                    (acc, false)
                } else if arg == "--phase" {
                    // Skip --phase and mark next arg to skip
                    (acc, true)
                } else if arg.starts_with("--phase=") {
                    // Skip --phase=value
                    (acc, false)
                } else {
                    acc.push(arg);
                    (acc, false)
                }
            })
            .0;

        // Add current phase to preserve the view
        args.push("--phase".to_string());
        args.push(self.current_phase.cli_name().to_string());

        // On Unix, use exec to replace the current process
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = Command::new(&exe).args(&args).exec();
            // exec only returns on error
            eprintln!("Failed to exec: {}", err);
            std::process::exit(1);
        }

        // On non-Unix, spawn a new process and exit
        #[cfg(not(unix))]
        {
            let _ = Command::new(&exe).args(&args).spawn();
            std::process::exit(0);
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        // Check if we're in THIR interactive sub-mode
        let in_thir_interactive = self.current_phase == CompilerPhase::Thir
            && self.compiler.thir_display_mode() == ThirDisplayMode::Interactive
            && self.thir_interactive_active;

        // Check if we're in HIR2 or TIR2 column browser
        let in_hir2_columns = self.current_phase == CompilerPhase::Hir2 && self.hir2_column_active;
        let in_tir2_columns = self.current_phase == CompilerPhase::Tir2 && self.tir2_column_active;
        let in_column_browser = in_hir2_columns || in_tir2_columns;

        // Check if we're on the VM Runner tab
        let in_vm_runner = self.current_phase == CompilerPhase::VmRunner;

        let ready_to_swap = matches!(self.rebuild_state, RebuildState::ReadyToSwap);

        match (key.code, key.modifiers) {
            // Quit on Ctrl+C or 'q'
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
            | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                self.should_quit = true;
            }
            // Toggle snapshot on 's'
            (KeyCode::Char('s'), KeyModifiers::NONE) => {
                self.toggle_snapshot();
            }
            // Delete snapshot on Shift+S
            (KeyCode::Char('S'), KeyModifiers::SHIFT) => {
                self.snapshot_files = None;
                self.snapshot_compiler = None;
                self.snapshot_parser_cache = None;
                self.scroll_offset = 0;
                self.last_compiled_files.clear();
                self.compile_current_state();
            }
            // Manual recompile on 'r'
            (KeyCode::Char('r'), KeyModifiers::NONE) => {
                self.recompile();
            }
            // Trigger compiler rebuild on 'R' (Shift+R)
            (KeyCode::Char('R'), KeyModifiers::SHIFT) => {
                if self.watcher.is_watching_compiler() {
                    self.trigger_rebuild();
                }
            }
            // Enter to swap to new binary when build is ready
            (KeyCode::Enter, KeyModifiers::NONE) if ready_to_swap => {
                self.exec_restart();
            }
            // Dismiss failed/ready-to-swap with Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                if ready_to_swap || matches!(self.rebuild_state, RebuildState::Failed(_)) {
                    self.rebuild_state = RebuildState::Idle;
                    self.rebuild_state_time = None;
                } else if self.thir_interactive_active {
                    self.thir_interactive_active = false;
                } else if in_hir2_columns {
                    self.hir2_column_active = false;
                } else if in_tir2_columns {
                    self.tir2_column_active = false;
                }
            }
            // Navigate phases with left/right arrow keys (only when not in interactive modes)
            (KeyCode::Left, _) => {
                if in_thir_interactive {
                    self.thir_cursor_left();
                } else if in_column_browser {
                    self.column_browser_left();
                } else {
                    self.current_phase = self.current_phase.prev();
                    self.scroll_offset = 0;
                    self.thir_interactive_active = false;
                }
            }
            (KeyCode::Right, _) => {
                if in_thir_interactive {
                    self.thir_cursor_right();
                } else if in_column_browser {
                    self.column_browser_right();
                } else {
                    self.current_phase = self.current_phase.next();
                    self.scroll_offset = 0;
                    self.thir_interactive_active = false;
                }
            }
            // Up/Down: scroll or navigate in interactive modes
            (KeyCode::Up, _) => {
                if in_thir_interactive {
                    self.thir_cursor_up();
                } else if in_column_browser {
                    self.column_browser_up();
                } else if in_vm_runner {
                    self.vm_runner_select_prev();
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
            }
            (KeyCode::Down, _) => {
                if in_thir_interactive {
                    self.thir_cursor_down();
                } else if in_column_browser {
                    self.column_browser_down();
                } else if in_vm_runner {
                    self.vm_runner_select_next();
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
            }
            // Page up/down — scroll detail pane when in column browser
            (KeyCode::PageUp, _) => {
                if in_column_browser {
                    self.column_browser_detail_scroll(-10);
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(10);
                }
            }
            (KeyCode::PageDown, _) => {
                if in_column_browser {
                    self.column_browser_detail_scroll(10);
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(10);
                }
            }
            // Home/End
            (KeyCode::Home, _) => {
                if in_column_browser {
                    self.column_browser_detail_scroll_home();
                } else {
                    self.scroll_offset = 0;
                }
            }
            (KeyCode::Char('m'), _) => {
                self.toggle_visualization_mode();
            }
            // 't' toggles interactive modes
            (KeyCode::Char('t'), KeyModifiers::NONE) => {
                if self.current_phase == CompilerPhase::Hir2 {
                    self.hir2_column_active = !self.hir2_column_active;
                } else if self.current_phase == CompilerPhase::Tir2 {
                    self.tir2_column_active = !self.tir2_column_active;
                } else if self.current_phase == CompilerPhase::Thir {
                    if self.compiler.thir_display_mode() == ThirDisplayMode::Interactive {
                        self.thir_interactive_active = !self.thir_interactive_active;
                    } else {
                        self.toggle_thir_display_mode();
                        self.thir_interactive_active = true;
                    }
                }
            }
            // Vim-style cursor navigation
            (KeyCode::Char('j'), KeyModifiers::NONE) => {
                if in_thir_interactive {
                    self.thir_cursor_down();
                } else if in_column_browser {
                    self.column_browser_down();
                } else if in_vm_runner {
                    self.vm_runner_select_next();
                }
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if in_thir_interactive {
                    self.thir_cursor_up();
                } else if in_column_browser {
                    self.column_browser_up();
                } else if in_vm_runner {
                    self.vm_runner_select_prev();
                }
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                if in_thir_interactive {
                    self.thir_cursor_left();
                } else if in_column_browser {
                    self.column_browser_left();
                }
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                if in_thir_interactive {
                    self.thir_cursor_right();
                } else if in_column_browser {
                    self.column_browser_right();
                }
            }
            // Execute function on Enter when on VM Runner tab
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if in_vm_runner {
                    self.vm_runner_execute();
                }
            }
            // Copy current output to clipboard with 'c' or 'y' (vim-style yank)
            (KeyCode::Char('c'), KeyModifiers::NONE) | (KeyCode::Char('y'), KeyModifiers::NONE) => {
                self.copy_to_clipboard();
            }
            // Paste from clipboard with 'p' (shows clipboard contents in a message)
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                self.paste_from_clipboard();
            }
            // Dismiss debug messages with 'd'
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                self.debug_messages.clear();
            }
            _ => {}
        }
    }

    fn toggle_visualization_mode(&mut self) {
        self.visualization_mode = match self.visualization_mode {
            VisualizationMode::Diff => VisualizationMode::Incremental,
            VisualizationMode::Incremental => VisualizationMode::Diff,
        };
    }

    fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) {
        let in_column_browser = (self.current_phase == CompilerPhase::Hir2
            && self.hir2_column_active)
            || (self.current_phase == CompilerPhase::Tir2 && self.tir2_column_active);
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if in_column_browser {
                    self.column_browser_detail_scroll(-3);
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if in_column_browser {
                    self.column_browser_detail_scroll(3);
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(3);
                }
            }
            _ => {}
        }
    }

    fn toggle_snapshot(&mut self) {
        // Save current files as snapshot (the "before" state)
        let snapshot_files = self.current_files.clone();

        // Capture the parser cache so we can compare against it on future compilations
        self.snapshot_parser_cache = Some(self.compiler.parser_cache_snapshot());

        // Create a separate compiler for the snapshot panel (fresh DB + new NodeCache)
        let snapshot_compiler = self.build_compiler_from_files(&snapshot_files, None);
        self.snapshot_compiler = Some(snapshot_compiler);

        self.snapshot_files = Some(snapshot_files);

        // Force a recompilation so incremental mode immediately reflects snapshot baseline
        self.last_compiled_files.clear();
        self.compile_current_state();
    }

    pub(crate) fn current_phase(&self) -> CompilerPhase {
        self.current_phase
    }

    pub(crate) fn current_output(&self) -> &str {
        self.compiler
            .get_phase_output(self.current_phase)
            .unwrap_or("No output available")
    }

    pub(crate) fn snapshot_output(&self) -> Option<&str> {
        self.snapshot_compiler
            .as_ref()
            .and_then(|c| c.get_phase_output(self.current_phase))
    }

    pub(crate) fn file_path(&self) -> &PathBuf {
        &self.file_path
    }

    pub(crate) fn has_snapshot(&self) -> bool {
        self.snapshot_compiler.is_some()
    }

    pub(crate) fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub(crate) fn get_recomputation_status(
        &self,
        phase: CompilerPhase,
    ) -> crate::compiler::RecomputationStatus {
        self.compiler.get_recomputation_status(phase)
    }

    pub(crate) fn get_output_annotated(
        &self,
        phase: CompilerPhase,
    ) -> Vec<(String, crate::compiler::LineStatus)> {
        self.compiler
            .get_annotated_output_with_mode(phase, self.visualization_mode)
    }

    pub(crate) fn get_snapshot_output_annotated(
        &self,
        phase: CompilerPhase,
    ) -> Option<Vec<(String, crate::compiler::LineStatus)>> {
        self.snapshot_compiler
            .as_ref()
            .map(|c| c.get_annotated_output_with_mode(phase, self.visualization_mode))
    }

    pub(crate) fn visualization_mode(&self) -> VisualizationMode {
        self.visualization_mode
    }

    pub(crate) fn visualization_mode_name(&self) -> &'static str {
        match self.visualization_mode {
            VisualizationMode::Diff => "Diff",
            VisualizationMode::Incremental => "Incremental",
        }
    }

    /// Get the current THIR display mode
    pub(crate) fn thir_display_mode(&self) -> ThirDisplayMode {
        self.compiler.thir_display_mode()
    }

    /// Check if THIR interactive mode is active
    pub(crate) fn thir_interactive_active(&self) -> bool {
        self.thir_interactive_active
    }

    pub(crate) fn hir2_column_active(&self) -> bool {
        self.hir2_column_active
    }

    pub(crate) fn hir2_column_data(&self) -> &crate::compiler::Hir2ColumnData {
        self.compiler.hir2_column_data()
    }

    pub(crate) fn hir2_column_state(&self) -> &crate::compiler::Hir2ColumnState {
        self.compiler.hir2_column_state()
    }

    pub(crate) fn tir2_column_active(&self) -> bool {
        self.tir2_column_active
    }

    pub(crate) fn tir2_column_data(&self) -> &crate::compiler::Tir2ColumnData {
        self.compiler.tir2_column_data()
    }

    pub(crate) fn tir2_column_state(&self) -> &crate::compiler::Tir2ColumnState {
        self.compiler.tir2_column_state()
    }

    /// Get the THIR interactive state for rendering
    pub(crate) fn thir_interactive_state(&self) -> &crate::compiler::ThirInteractiveState {
        self.compiler.thir_interactive_state()
    }

    /// Toggle THIR display mode between Tree and Interactive
    fn toggle_thir_display_mode(&mut self) {
        let new_mode = match self.compiler.thir_display_mode() {
            ThirDisplayMode::Tree => {
                self.thir_interactive_active = true;
                self.compiler.format_thir_interactive();
                ThirDisplayMode::Interactive
            }
            ThirDisplayMode::Interactive => {
                self.thir_interactive_active = false;
                ThirDisplayMode::Tree
            }
        };
        self.compiler.set_thir_display_mode(new_mode);
    }

    /// Move THIR cursor down
    fn thir_cursor_down(&mut self) {
        let state = self.compiler.thir_interactive_state_mut();
        if state.cursor_line + 1 < state.total_lines {
            state.cursor_line += 1;
        }
    }

    /// Move THIR cursor up
    fn thir_cursor_up(&mut self) {
        let state = self.compiler.thir_interactive_state_mut();
        if state.cursor_line > 0 {
            state.cursor_line -= 1;
        }
    }

    /// Move THIR cursor left
    fn thir_cursor_left(&mut self) {
        let state = self.compiler.thir_interactive_state_mut();
        if state.cursor_col > 0 {
            state.cursor_col -= 1;
        }
    }

    /// Move THIR cursor right
    fn thir_cursor_right(&mut self) {
        let state = self.compiler.thir_interactive_state_mut();
        let max_col = state
            .source_lines
            .get(state.cursor_line)
            .map(|l| l.len())
            .unwrap_or(0);
        if state.cursor_col + 1 < max_col {
            state.cursor_col += 1;
        }
    }

    // ── HIR2 Column Navigation ─────────────────────────────────────

    fn hir2_column_up(&mut self) {
        let state = self.compiler.hir2_column_state_mut();
        let col = state.active_column;
        if state.selected[col] > 0 {
            state.selected[col] -= 1;
            state.detail_scroll = 0;
            // Reset downstream selections when changing upstream
            for i in (col + 1)..3 {
                state.selected[i] = 0;
            }
        }
    }

    fn hir2_column_down(&mut self) {
        let data = self.compiler.hir2_column_data().clone();
        let state = self.compiler.hir2_column_state_mut();
        let col = state.active_column;
        let max = match col {
            0 => data.packages.len(),
            1 => data
                .packages
                .get(state.selected[0])
                .map(|p| p.files.len())
                .unwrap_or(0),
            2 => data
                .packages
                .get(state.selected[0])
                .and_then(|p| p.files.get(state.selected[1]))
                .map(|f| f.items.len())
                .unwrap_or(0),
            _ => 0,
        };
        if max > 0 && state.selected[col] + 1 < max {
            state.selected[col] += 1;
            state.detail_scroll = 0;
            for i in (col + 1)..3 {
                state.selected[i] = 0;
            }
        }
    }

    fn hir2_column_left(&mut self) {
        let state = self.compiler.hir2_column_state_mut();
        if state.active_column > 0 {
            state.active_column -= 1;
            state.detail_scroll = 0;
        }
    }

    fn hir2_column_right(&mut self) {
        let data = self.compiler.hir2_column_data().clone();
        let state = self.compiler.hir2_column_state_mut();
        let can_go_right = match state.active_column {
            0 => !data.packages.is_empty() && !data.packages[state.selected[0]].files.is_empty(),
            1 => data
                .packages
                .get(state.selected[0])
                .and_then(|p| p.files.get(state.selected[1]))
                .map(|f| !f.items.is_empty())
                .unwrap_or(false),
            _ => false,
        };
        if can_go_right && state.active_column < 2 {
            state.active_column += 1;
            state.detail_scroll = 0;
        }
    }

    // ── Unified column browser helpers (delegates to HIR2 or TIR2) ──────────

    fn column_browser_up(&mut self) {
        if self.current_phase == CompilerPhase::Hir2 {
            self.hir2_column_up();
        } else {
            self.tir2_column_up();
        }
    }

    fn column_browser_down(&mut self) {
        if self.current_phase == CompilerPhase::Hir2 {
            self.hir2_column_down();
        } else {
            self.tir2_column_down();
        }
    }

    fn column_browser_left(&mut self) {
        if self.current_phase == CompilerPhase::Hir2 {
            self.hir2_column_left();
        } else {
            self.tir2_column_left();
        }
    }

    fn column_browser_right(&mut self) {
        if self.current_phase == CompilerPhase::Hir2 {
            self.hir2_column_right();
        } else {
            self.tir2_column_right();
        }
    }

    fn column_browser_detail_scroll(&mut self, delta: i32) {
        if self.current_phase == CompilerPhase::Hir2 {
            let state = self.compiler.hir2_column_state_mut();
            if delta > 0 {
                state.detail_scroll = state.detail_scroll.saturating_add(delta as usize);
            } else {
                state.detail_scroll = state.detail_scroll.saturating_sub((-delta) as usize);
            }
        } else {
            let state = self.compiler.tir2_column_state_mut();
            if delta > 0 {
                state.detail_scroll = state.detail_scroll.saturating_add(delta as usize);
            } else {
                state.detail_scroll = state.detail_scroll.saturating_sub((-delta) as usize);
            }
        }
    }

    fn column_browser_detail_scroll_home(&mut self) {
        if self.current_phase == CompilerPhase::Hir2 {
            self.compiler.hir2_column_state_mut().detail_scroll = 0;
        } else {
            self.compiler.tir2_column_state_mut().detail_scroll = 0;
        }
    }

    // ── TIR2 column browser navigation ──────────────────────────────────────

    fn tir2_column_up(&mut self) {
        let state = self.compiler.tir2_column_state_mut();
        let col = state.active_column;
        if state.selected[col] > 0 {
            state.selected[col] -= 1;
            state.detail_scroll = 0;
            for i in (col + 1)..3 {
                state.selected[i] = 0;
            }
        }
    }

    fn tir2_column_down(&mut self) {
        let data = self.compiler.tir2_column_data().clone();
        let state = self.compiler.tir2_column_state_mut();
        let col = state.active_column;
        let max = match col {
            0 => data.packages.len(),
            1 => data
                .packages
                .get(state.selected[0])
                .map(|p| p.files.len())
                .unwrap_or(0),
            2 => data
                .packages
                .get(state.selected[0])
                .and_then(|p| p.files.get(state.selected[1]))
                .map(|f| f.items.len())
                .unwrap_or(0),
            _ => 0,
        };
        if max > 0 && state.selected[col] + 1 < max {
            state.selected[col] += 1;
            state.detail_scroll = 0;
            for i in (col + 1)..3 {
                state.selected[i] = 0;
            }
        }
    }

    fn tir2_column_left(&mut self) {
        let state = self.compiler.tir2_column_state_mut();
        if state.active_column > 0 {
            state.active_column -= 1;
            state.detail_scroll = 0;
        }
    }

    fn tir2_column_right(&mut self) {
        let data = self.compiler.tir2_column_data().clone();
        let state = self.compiler.tir2_column_state_mut();
        let can_go_right = match state.active_column {
            0 => !data.packages.is_empty() && !data.packages[state.selected[0]].files.is_empty(),
            1 => data
                .packages
                .get(state.selected[0])
                .and_then(|p| p.files.get(state.selected[1]))
                .map(|f| !f.items.is_empty())
                .unwrap_or(false),
            _ => false,
        };
        if can_go_right && state.active_column < 2 {
            state.active_column += 1;
            state.detail_scroll = 0;
        }
    }

    /// VM Runner: select previous function
    fn vm_runner_select_prev(&mut self) {
        let state = self.compiler.vm_runner_state_mut();
        if state.selected_function > 0 {
            state.selected_function -= 1;
            // Clear execution result when changing selection
            state.execution_result = None;
        }
        // Regenerate output to show updated selection
        self.compiler.run_single_phase(CompilerPhase::VmRunner);
    }

    /// VM Runner: select next function
    fn vm_runner_select_next(&mut self) {
        let state = self.compiler.vm_runner_state_mut();
        let max = state.available_functions.len().saturating_sub(1);
        if state.selected_function < max {
            state.selected_function += 1;
            // Clear execution result when changing selection
            state.execution_result = None;
        }
        // Regenerate output to show updated selection
        self.compiler.run_single_phase(CompilerPhase::VmRunner);
    }

    /// VM Runner: execute selected function
    fn vm_runner_execute(&mut self) {
        self.compiler.execute_selected_function();
    }

    fn compile_current_state(&mut self) {
        if self.current_files == self.last_compiled_files {
            return;
        }

        if let Some(snapshot_cache) = &self.snapshot_parser_cache {
            // Compare current filesystem against the frozen snapshot baseline
            self.compiler.set_parser_cache_baseline(snapshot_cache);
            self.compiler
                .compile_from_filesystem(&self.current_files, self.snapshot_files.as_ref());
            // Restore baseline so the next run still uses the snapshot cache
            self.compiler.set_parser_cache_baseline(snapshot_cache);
        } else {
            // No snapshot: keep reusing the same compiler/NodeCache to accumulate reuse info
            self.compiler
                .compile_from_filesystem(&self.current_files, None);
        }

        // Collect any debug messages emitted during compilation
        self.debug_messages = baml_base::drain_debug_log();

        self.last_compiled_files = self.current_files.clone();
    }

    fn build_compiler_from_files(
        &self,
        files: &HashMap<PathBuf, String>,
        snapshot: Option<&HashMap<PathBuf, String>>,
    ) -> CompilerRunner {
        let mut compiler = CompilerRunner::new(&self.file_path);
        compiler.compile_from_filesystem(files, snapshot);
        compiler
    }

    /// Copy the current output to the system clipboard
    fn copy_to_clipboard(&mut self) {
        let output = self.current_output().to_string();

        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(&output) {
                Ok(()) => {
                    self.last_copy_time = Some(Instant::now());
                    self.clipboard_error = None;
                }
                Err(e) => {
                    self.clipboard_error = Some(format!("Failed to copy: {}", e));
                    self.last_copy_time = Some(Instant::now());
                }
            },
            Err(e) => {
                self.clipboard_error = Some(format!("Clipboard unavailable: {}", e));
                self.last_copy_time = Some(Instant::now());
            }
        }
    }

    /// Paste from clipboard (displays clipboard contents as a status message)
    fn paste_from_clipboard(&mut self) {
        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.get_text() {
                Ok(text) => {
                    // For now, just show feedback that paste was read
                    // In future, this could be used for search or filtering
                    let preview = if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text
                    };
                    self.clipboard_error = Some(format!("Clipboard: {}", preview));
                    self.last_copy_time = Some(Instant::now());
                }
                Err(e) => {
                    self.clipboard_error = Some(format!("Failed to paste: {}", e));
                    self.last_copy_time = Some(Instant::now());
                }
            },
            Err(e) => {
                self.clipboard_error = Some(format!("Clipboard unavailable: {}", e));
                self.last_copy_time = Some(Instant::now());
            }
        }
    }

    /// Check if the "Copied!" feedback should be shown
    pub(crate) fn show_copy_feedback(&self) -> bool {
        self.last_copy_time
            .map(|t| t.elapsed() < COPY_FEEDBACK_DURATION)
            .unwrap_or(false)
    }

    /// Get the clipboard status message (either "Copied!" or an error)
    pub(crate) fn clipboard_status(&self) -> Option<&str> {
        if self.show_copy_feedback() {
            if let Some(ref error) = self.clipboard_error {
                Some(error.as_str())
            } else {
                Some("Copied to clipboard!")
            }
        } else {
            None
        }
    }

    /// Get the current rebuild state
    pub(crate) fn rebuild_state(&self) -> &RebuildState {
        &self.rebuild_state
    }

    /// Check if compiler hot-reload is enabled
    pub(crate) fn is_hot_reload_enabled(&self) -> bool {
        self.watcher.is_watching_compiler()
    }

    /// Get debug messages collected from the compiler
    pub(crate) fn debug_messages(&self) -> &[DebugMessage] {
        &self.debug_messages
    }

    /// Get watcher diagnostic summary for display in the UI
    pub(crate) fn watcher_diagnostic_summary(&self) -> String {
        self.watcher.diagnostic_summary()
    }
}
