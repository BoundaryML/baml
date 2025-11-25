use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    compiler::{
        CompilerPhase, CompilerRunner, GreenElementId, VisualizationMode, read_files_from_disk,
    },
    ui,
    watcher::FileWatcher,
};

pub(crate) struct App {
    file_path: PathBuf,
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
}

impl App {
    pub(crate) fn new(path: PathBuf) -> Result<Self> {
        let watcher = FileWatcher::new(&path)?;
        let mut compiler = CompilerRunner::new(&path);

        // Read initial files from disk
        let current_files = read_files_from_disk(&path)?;
        let initial_files = current_files.clone();

        // Initial compilation (no snapshot)
        compiler.compile_from_filesystem(&current_files, None);

        Ok(Self {
            file_path: path,
            current_phase: CompilerPhase::Lexer,
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
        })
    }

    pub(crate) fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        while !self.should_quit {
            // Check for file changes
            if self.watcher.check_for_changes() {
                self.reload_file()?;
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

    fn handle_key_event(&mut self, key: KeyEvent) {
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
            // Navigate phases with left/right arrow keys
            (KeyCode::Left, _) => {
                self.current_phase = self.current_phase.prev();
                self.scroll_offset = 0;
            }
            (KeyCode::Right, _) => {
                self.current_phase = self.current_phase.next();
                self.scroll_offset = 0;
            }
            // Scroll with up/down arrow keys
            (KeyCode::Up, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            (KeyCode::Down, _) => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            // Page up/down
            (KeyCode::PageUp, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            (KeyCode::PageDown, _) => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            // Home/End
            (KeyCode::Home, _) => {
                self.scroll_offset = 0;
            }
            (KeyCode::Char('m'), _) => {
                self.toggle_visualization_mode();
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
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(3);
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
}
