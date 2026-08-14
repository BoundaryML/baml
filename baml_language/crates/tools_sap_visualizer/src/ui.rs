use ::bex_sap::deserializer::coercer::ParsingError;
use ::eframe::egui::{
    self, Color32, Key, Rect, RichText, TextBuffer,
    text::{CCursor, CCursorRange, LayoutSection, TextWrapping},
};
use ::std::borrow::Cow;

use crate::{compile, state::SapVisualizerState};

const DEFAULT_BAML_SOURCE: &str = r#"enum Degree {
  HighSchool
  Associate
  Bachelor
  Master
  Doctorate @alias("PhD")
}

class Education {
  school string
  degree Degree
  year int?
}

class Resume {
  name string
  email string
  phone string?
  experience int[]
  education Education[]
  skills string[]
}"#;

const DEFAULT_TYPE_EXPR: &str = "Resume";

/// Result received from the background compilation thread.
type CompileResult = Result<compile::CompiledSapModel, String>;

pub struct SapVisualizer {
    /// The BAML source code (class/enum definitions).
    baml_source: String,
    /// The type expression to parse as (e.g. `Resume`, `string | int`).
    type_expr: String,
    /// Compilation status.
    compile_status: CompileStatus,
    /// The SAP visualizer state (None if compilation failed).
    sap: Option<SapVisualizerState>,
    text_highlight: Option<(usize, usize)>,
    /// Stack of auto-inserted closers to the right of the cursor (innermost
    /// first).  When the user types a closer matching the top of the stack,
    /// we over-type instead of inserting.  Cleared on unexpected cursor moves.
    overtype_stack: AutoCloseStack,
    /// Channel for receiving compilation results from the background thread.
    compile_rx: Option<std::sync::mpsc::Receiver<CompileResult>>,
    /// Repaint context for waking the UI when compilation finishes.
    ctx: Option<egui::Context>,
    /// Timestamp of the last editor change, for debouncing recompilation.
    last_change: Option<web_time::Instant>,
}

enum CompileStatus {
    Ok,
    Compiling,
    Error(String),
}

impl SapVisualizer {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let baml_source = DEFAULT_BAML_SOURCE.to_string();
        let type_expr = DEFAULT_TYPE_EXPR.to_string();
        // Initial compile is synchronous so we have something to show immediately.
        let (sap, compile_status) = Self::try_compile_sync(&baml_source, &type_expr);
        Self {
            baml_source,
            type_expr,
            compile_status,
            sap,
            text_highlight: None,
            overtype_stack: AutoCloseStack::default(),
            compile_rx: None,
            ctx: None,
            last_change: None,
        }
    }

    fn try_compile_sync(
        baml_source: &str,
        type_expr: &str,
    ) -> (Option<SapVisualizerState>, CompileStatus) {
        match compile::compile_baml_to_sap(baml_source, type_expr) {
            Ok(compiled) => {
                let sap = SapVisualizerState::new(String::default(), compiled);
                (Some(sap), CompileStatus::Ok)
            }
            Err(e) => (None, CompileStatus::Error(e)),
        }
    }

    fn recompile(&mut self) {
        self.compile_status = CompileStatus::Compiling;
        let baml_source = self.baml_source.clone();
        let type_expr = self.type_expr.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.compile_rx = Some(rx);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = compile::compile_baml_to_sap(&baml_source, &type_expr);
            let _ = tx.send(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }

    /// Poll for a completed background compilation and apply it.
    fn poll_compile(&mut self) {
        let Some(ref rx) = self.compile_rx else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.compile_rx = None;
                self.compile_status =
                    CompileStatus::Error("Background compilation crashed".to_string());
                return;
            }
        };
        self.compile_rx = None;
        let old_json = self.sap.as_ref().map(|s| s.json().to_string());
        match result {
            Ok(compiled) => {
                let mut sap = SapVisualizerState::new(String::default(), compiled);
                if let Some(json) = old_json
                    && !json.is_empty()
                {
                    sap.update_with_json(json);
                }

                self.sap = Some(sap);
                self.compile_status = CompileStatus::Ok;
            }
            Err(e) => {
                self.compile_status = CompileStatus::Error(e);
                // Keep the old sap so the user can still see the previous output.
            }
        }
    }
}

const JSON_INPUT_FORMAT: egui::TextFormat = egui::TextFormat {
    font_id: egui::FontId::monospace(14.0),
    extra_letter_spacing: 0.0,
    line_height: None,
    color: egui::Color32::WHITE,
    background: egui::Color32::TRANSPARENT,
    expand_bg: 1.0,
    italics: false,
    underline: egui::Stroke::NONE,
    strikethrough: egui::Stroke::NONE,
    valign: egui::Align::BOTTOM,
};
const JSON_INPUT_FORMAT_HIGHLIGHT: egui::TextFormat = egui::TextFormat {
    font_id: egui::FontId::monospace(14.0),
    extra_letter_spacing: 0.0,
    line_height: None,
    color: egui::Color32::BLACK,
    background: egui::Color32::CYAN,
    expand_bg: 1.0,
    italics: false,
    underline: egui::Stroke::NONE,
    strikethrough: egui::Stroke::NONE,
    valign: egui::Align::BOTTOM,
};

impl eframe::App for SapVisualizer {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.ctx = Some(ctx.clone());
        self.poll_compile();

        // Left panel: BAML source + type expression editors.
        egui::SidePanel::left("baml_panel")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label("BAML Source:");
                    let baml_response = egui::TextEdit::multiline(&mut self.baml_source)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .hint_text("class Foo { ... }")
                        .show(ui);

                    ui.add_space(8.0);
                    ui.label("Parse as type:");
                    let type_response = egui::TextEdit::singleline(&mut self.type_expr)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .hint_text("Resume")
                        .show(ui);

                    if baml_response.response.changed() || type_response.response.changed() {
                        self.last_change = Some(web_time::Instant::now());
                    }
                    // Debounce: recompile only after 300ms of no changes.
                    if let Some(last) = self.last_change {
                        if last.elapsed() >= std::time::Duration::from_millis(300) {
                            self.last_change = None;
                            self.recompile();
                        } else if let Some(ctx) = &self.ctx {
                            ctx.request_repaint_after(std::time::Duration::from_millis(300));
                        }
                    }

                    ui.add_space(8.0);
                    match &self.compile_status {
                        CompileStatus::Ok => {
                            ui.colored_label(Color32::GREEN, "✓ Compiled");
                        }
                        CompileStatus::Compiling => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Compiling…");
                            });
                        }
                        CompileStatus::Error(err) => {
                            ui.colored_label(Color32::RED, RichText::new(err).monospace());
                        }
                    }
                });
            });

        // Top panel: JSON input.
        eframe::egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.take_available_width();
            ui.vertical(|ui| {
                ui.take_available_width();
                ui.label("JSON:");
                if let Some(ref mut sap) = self.sap {
                    let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, _: f32| {
                        let sections = match self.text_highlight {
                            Some((start, end)) => {
                                assert!(start <= end);
                                let mut sections = Vec::new();
                                if start > 0 {
                                    sections.push(egui::text::LayoutSection {
                                        leading_space: 0.0,
                                        byte_range: 0..start,
                                        format: JSON_INPUT_FORMAT,
                                    });
                                }
                                if start < end && start < buf.as_str().len() {
                                    sections.push(egui::text::LayoutSection {
                                        leading_space: 0.0,
                                        byte_range: start..std::cmp::min(end, buf.as_str().len()),
                                        format: JSON_INPUT_FORMAT_HIGHLIGHT,
                                    });
                                }
                                if end < buf.as_str().len() {
                                    sections.push(egui::text::LayoutSection {
                                        leading_space: 0.0,
                                        byte_range: end..buf.as_str().len(),
                                        format: JSON_INPUT_FORMAT,
                                    });
                                }
                                sections
                            }
                            None => vec![LayoutSection {
                                leading_space: 0.0,
                                byte_range: 0..buf.as_str().len(),
                                format: JSON_INPUT_FORMAT,
                            }],
                        };
                        let layout_job = egui::text::LayoutJob {
                            text: buf.as_str().to_string(),
                            sections,
                            wrap: TextWrapping::no_max_width(),
                            first_row_min_height: 0.0,
                            break_on_newline: true,
                            halign: egui::emath::Align::LEFT,
                            justify: false,
                            round_output_to_gui: false,
                        };
                        ui.fonts_mut(|f| f.layout_job(layout_job))
                    };
                    let text_edit_id = ui.make_persistent_id("json_input");
                    let action =
                        preprocess_json_input(ui, text_edit_id, sap, &mut self.overtype_stack);

                    let output = egui::TextEdit::multiline(sap)
                        .code_editor()
                        .id(text_edit_id)
                        .desired_width(f32::INFINITY)
                        .hint_text("Put some JSON here")
                        .layouter(&mut layouter)
                        .show(ui);

                    // After show(): adjust cursor or shift the stack for edits
                    // that egui processed (normal typing, backspace, etc.).
                    match action {
                        PostShowAction::AdjustCursor(delta) => {
                            if let Some(cr) = output.cursor_range {
                                let new_idx = (cr.primary.index as i32 + delta) as usize;
                                let mut state = output.state.clone();
                                state
                                    .cursor
                                    .set_char_range(Some(CCursorRange::one(CCursor::new(new_idx))));
                                state.store(ui.ctx(), output.response.id);
                            }
                        }
                        PostShowAction::AlreadyHandled => {
                            // We handled the edit before show(); nothing more to do.
                        }
                        PostShowAction::None(old_cursor) => {
                            // egui may have processed normal typing or other edits.
                            // Shift the stack to keep indices in sync.
                            if output.response.changed()
                                && let Some(cr) = output.cursor_range
                            {
                                let new_cursor = cr.primary.index;
                                if new_cursor > old_cursor {
                                    let inserted = new_cursor - old_cursor;
                                    self.overtype_stack.shift_for_insert(old_cursor, inserted);
                                } else if new_cursor < old_cursor {
                                    let deleted = old_cursor - new_cursor;
                                    self.overtype_stack.shift_for_delete(new_cursor, deleted);
                                } else {
                                    self.overtype_stack.clear();
                                }
                            }
                        }
                    }
                } else {
                    ui.colored_label(Color32::GRAY, "Fix compilation errors to enable JSON input");
                }
            });
        });

        // Central panel: output grid.
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            let Some(ref sap) = self.sap else {
                ui.colored_label(Color32::GRAY, "No output — fix compilation errors above");
                return;
            };
            ui.take_available_space();
            ui.vertical(|ui| {
                ui.take_available_space();
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("output").show(ui, |ui| {
                        let first_item = RowData::make_item(sap.iter().next().unwrap_or(&None));
                        let mut last_real_value: Option<String> = if first_item.0 == Color32::WHITE
                        {
                            Some(first_item.1.to_string())
                        } else {
                            None
                        };
                        let mut prev = RowData {
                            sap,
                            byte_idx: 0,
                            text: "".to_string(),
                            text_len: 0,
                            item: first_item,
                            diff: None,
                        };
                        let mut pointer = None;
                        ctx.input(|i| pointer = i.pointer.hover_pos());
                        self.text_highlight = None;
                        for ((byte_idx, c), item) in std::iter::zip(
                            sap.json()
                                .char_indices()
                                .chain(std::iter::once((sap.json().len(), '\0'))),
                            sap.iter().skip(1),
                        ) {
                            let item = RowData::make_item(item);
                            if prev.item == item {
                                prev.add_char(c);
                            } else {
                                let would_be_highlight =
                                    (prev.byte_idx, prev.byte_idx + prev.text_len);
                                let row_rect = prev.render_row(ui);
                                if pointer.is_some_and(|pointer| row_rect.contains(pointer)) {
                                    self.text_highlight = Some(would_be_highlight);
                                }
                                let diff = if item.0 == Color32::WHITE {
                                    let d = last_real_value
                                        .as_deref()
                                        .map(|prev_val| compute_item_diff(prev_val, &item.1));
                                    last_real_value = Some(item.1.to_string());
                                    d
                                } else {
                                    None
                                };
                                prev = RowData {
                                    sap,
                                    byte_idx,
                                    text: c.to_string(),
                                    text_len: c.len_utf8(),
                                    item,
                                    diff,
                                };
                            }
                        }
                        let would_be_highlight = (prev.byte_idx, prev.byte_idx + prev.text_len);
                        let row_rect = prev.render_row(ui);
                        if pointer.is_some_and(|pointer| row_rect.contains(pointer)) {
                            self.text_highlight = Some(would_be_highlight);
                        }
                    });
                });
            });
        });
    }
}

/// Build the string to insert when Enter is pressed at `cursor` (char index)
/// in `text`. Returns `"\n"` plus matching indentation, with an extra indent
/// level if the line ends with `{` or `[`.
fn newline_with_indent(text: &str, cursor: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let before = &chars[..cursor];

    let line_start = before.iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let indent: String = before[line_start..]
        .iter()
        .take_while(|c| c.is_ascii_whitespace())
        .collect();

    let last_non_ws = before.iter().rposition(|c| !c.is_ascii_whitespace());
    let ends_with_opener = last_non_ws.is_some_and(|i| matches!(before[i], '{' | '['));

    let char_after_cursor = chars.get(cursor).copied();

    if ends_with_opener {
        let opener = before[last_non_ws.unwrap()];
        let closer = if opener == '{' { '}' } else { ']' };
        if char_after_cursor == Some(closer) {
            // Cursor between matched pair: `{|}` → `{\n\t|\n}`
            // we use tabs not spaces here since we want to minimize the size of the printed partials in the table below
            return format!("\n{indent}\t\n{indent}");
        }
        return format!("\n{indent}\t");
    }

    format!("\n{indent}")
}

/// If `typed` is an opener character (`{`, `[`, `"`), return a replacement
/// string with the closer appended (e.g. `"{}"`) so egui inserts both in one
/// atomic operation. Returns `None` if no auto-close should fire.
///
/// `text` is the current buffer content and `cursor` is the char-index where
/// the text will be inserted.
/// Walk `chars[..cursor]` and determine whether the cursor sits inside a
/// JSON string literal.  Handles backslash escapes correctly so that `\"`
/// does not flip the in-string state.
fn cursor_inside_string(chars: &[char], cursor: usize) -> bool {
    let mut in_string = false;
    let mut i = 0;
    while i < cursor {
        match chars[i] {
            '"' => in_string = !in_string,
            '\\' if in_string => {
                i += 1; // skip the escaped character
            }
            _ => {}
        }
        i += 1;
    }
    in_string
}

fn auto_close_text(text: &str, cursor: usize, typed: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let inside_string = cursor_inside_string(&chars, cursor);

    let closer = match typed {
        // Don't auto-close braces/brackets when inside a string literal.
        "{" if !inside_string => "}",
        "[" if !inside_string => "]",
        "\"" => {
            let after = chars.get(cursor).copied();
            if after == Some('"') {
                return None;
            }
            if inside_string {
                return None;
            }
            "\""
        }
        _ => return None,
    };
    Some(format!("{typed}{closer}"))
}

/// The action that `preprocess_json_input` asks the caller to perform after
/// `show()`.
enum PostShowAction {
    /// No action needed.  The `usize` is the cursor position before `show()`,
    /// used to detect edits egui processed and shift the stack accordingly.
    None(usize),
    /// Move the cursor by this signed offset from where egui left it.
    AdjustCursor(i32),
    /// We already handled the edit ourselves (Enter / pair-delete); egui's
    /// `show()` should see the already-modified buffer and not process any
    /// mutating events.
    AlreadyHandled,
}

/// Pre-process egui input events for JSON editing assistance (auto-close,
/// over-type, pair-delete, and auto-indent). Mutates the event queue and text
/// buffer as needed so that edits are atomic from the undoer's perspective.
fn preprocess_json_input(
    ui: &mut egui::Ui,
    text_edit_id: egui::Id,
    sap: &mut SapVisualizerState,
    stack: &mut AutoCloseStack,
) -> PostShowAction {
    // Only process events when the JSON editor has focus.
    if !ui.ctx().memory(|mem| mem.has_focus(text_edit_id)) {
        stack.clear();
        return PostShowAction::None(0);
    }
    let Some(state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) else {
        return PostShowAction::None(0);
    };
    let Some(cursor_range) = state.cursor.char_range() else {
        return PostShowAction::None(0);
    };
    let cursor = cursor_range.primary.index;
    let text = sap.as_str();

    // Invalidate entries whose enclosing range no longer contains the cursor.
    stack.validate_cursor(cursor);

    // --- Backspace: pair-delete if the cursor is right after an auto-closed
    // opener ---
    let has_backspace = ui.input_mut(|input| {
        input.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: Key::Backspace,
                    pressed: true,
                    ..
                }
            )
        })
    });
    if has_backspace && let Some(closer_idx) = stack.try_pair_delete(cursor) {
        // Remove the backspace event — we handle it ourselves.
        ui.input_mut(|input| {
            input.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key {
                        key: Key::Backspace,
                        pressed: true,
                        ..
                    }
                )
            });
        });
        // Delete both opener (cursor - 1) and closer.
        // Delete closer first (higher index) so the opener index stays valid.
        sap.delete_char_range(closer_idx..closer_idx + 1);
        stack.shift_for_delete(closer_idx, 1);
        sap.delete_char_range(cursor - 1..cursor);
        stack.shift_for_delete(cursor - 1, 1);
        let mut state = state;
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(cursor - 1))));
        state.store(ui.ctx(), text_edit_id);
        return PostShowAction::AlreadyHandled;
    }

    // --- Text events: over-type or auto-close ---
    let mut cursor_adjust: i32 = 0;
    ui.input_mut(|input| {
        for event in &mut input.events {
            if let egui::Event::Text(t) = event {
                // Over-type: if the typed char matches an auto-inserted closer
                // at the cursor, consume the event and advance cursor.
                if t.len() == 1 {
                    let ch = t.chars().next().unwrap();
                    if stack.try_overtype(cursor, ch) {
                        t.clear();
                        cursor_adjust = 1;
                        continue;
                    }
                }
                // Auto-close: rewrite opener to opener+closer.
                if let Some(replacement) = auto_close_text(text, cursor, t) {
                    let closer = replacement.chars().last().unwrap();
                    let insert_len = replacement.chars().count();
                    // Shift existing entries to account for the text egui
                    // will insert at `cursor`, then push the new entry with
                    // its final absolute indices.
                    stack.shift_for_insert(cursor, insert_len);
                    stack.push(cursor, cursor + insert_len - 1, closer);
                    cursor_adjust = -(replacement.len() as i32 - t.len() as i32);
                    *t = replacement;
                }
            }
        }
    });
    if cursor_adjust != 0 {
        return PostShowAction::AdjustCursor(cursor_adjust);
    }

    // --- Enter: auto-indent ---
    let enter_pressed = ui.input_mut(|input| {
        let had = input.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    ..
                }
            )
        });
        if had {
            input.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        ..
                    }
                )
            });
        }
        had
    });

    if enter_pressed {
        let insert = newline_with_indent(text, cursor);
        let insert_len = insert.chars().count();
        // For the `{|\n}` case the cursor lands on the middle line, not at
        // the very end of the inserted text.
        let cursor_pos = if insert.matches('\n').count() == 2 {
            let second_nl = insert.find('\n').unwrap()
                + 1
                + insert[insert.find('\n').unwrap() + 1..].find('\n').unwrap();
            cursor + second_nl
        } else {
            cursor + insert_len
        };
        sap.insert_text(&insert, cursor);
        stack.shift_for_insert(cursor, insert_len);
        let mut state = state;
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(cursor_pos))));
        state.store(ui.ctx(), text_edit_id);
        return PostShowAction::AlreadyHandled;
    }

    PostShowAction::None(cursor)
}

/// Tracks auto-inserted closers for over-type and pair-delete behavior,
/// modelled after VSCode's `AutoClosedAction`.
///
/// Each entry records the char-index of the closer and the enclosing range
/// (opener..=closer).  When text is inserted or deleted *before* the closer
/// the indices shift accordingly.  An entry is invalidated when the cursor
/// leaves the enclosing range.
///
/// Reference: `src/vs/editor/common/cursor/cursor.ts` — `AutoClosedAction`,
/// `isAutoClosingOvertype`, `_runAutoClosingPairDelete` in the VSCode repo.
#[derive(Default)]
struct AutoCloseStack {
    entries: Vec<AutoCloseEntry>,
}

#[derive(Clone, Copy)]
struct AutoCloseEntry {
    /// Char-index of the auto-inserted closer.
    closer_idx: usize,
    /// The closer character (e.g. `}`, `]`, `"`).
    closer_char: char,
    /// Char-index of the opener (inclusive start of enclosing range).
    open_idx: usize,
}

impl AutoCloseStack {
    /// Record that we auto-inserted `closer_char` when the user typed an
    /// opener at `open_idx`.  The closer lands at `closer_idx`.
    fn push(&mut self, open_idx: usize, closer_idx: usize, closer_char: char) {
        self.entries.push(AutoCloseEntry {
            closer_idx,
            closer_char,
            open_idx,
        });
    }

    /// If `typed` is a closer that we auto-inserted at `cursor`, consume it
    /// (over-type).  Returns `true` if the caller should skip normal insertion
    /// and advance the cursor by 1.
    fn try_overtype(&mut self, cursor: usize, typed: char) -> bool {
        // Find the innermost matching entry (last in the vec).
        let pos = self
            .entries
            .iter()
            .rposition(|e| e.closer_idx == cursor && e.closer_char == typed);
        if let Some(i) = pos {
            self.entries.remove(i);
            // The over-type replaces the closer in-place (no net text change),
            // but conceptually the cursor advances past it.  Shift any entries
            // whose closer was at the same position (shouldn't happen in
            // practice since closers are unique positions).
            return true;
        }
        false
    }

    /// If the cursor is right after an opener whose closer was auto-inserted,
    /// return the closer index so the caller can delete both.  This implements
    /// VSCode's "backspace deletes the pair" behaviour.
    fn try_pair_delete(&mut self, cursor: usize) -> Option<usize> {
        // cursor is the char-index *after* the opener, i.e. `open_idx + 1`.
        let pos = self.entries.iter().rposition(|e| e.open_idx + 1 == cursor);
        if let Some(i) = pos {
            let closer_idx = self.entries[i].closer_idx;
            self.entries.remove(i);
            return Some(closer_idx);
        }
        None
    }

    /// Adjust all tracked indices after text of length `len` (in chars) was
    /// inserted at char-index `at`.
    fn shift_for_insert(&mut self, at: usize, len: usize) {
        for e in &mut self.entries {
            if e.open_idx >= at {
                e.open_idx += len;
            }
            if e.closer_idx >= at {
                e.closer_idx += len;
            }
        }
    }

    /// Adjust all tracked indices after `len` chars were deleted starting at
    /// char-index `at`.
    fn shift_for_delete(&mut self, at: usize, len: usize) {
        self.entries.retain_mut(|e| {
            // If the deletion overlaps the closer, invalidate.
            if at <= e.closer_idx && e.closer_idx < at + len {
                return false;
            }
            if e.open_idx >= at + len {
                e.open_idx -= len;
            } else if e.open_idx > at {
                e.open_idx = at;
            }
            if e.closer_idx >= at + len {
                e.closer_idx -= len;
            }
            true
        });
    }

    /// Remove all entries.
    fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove entries whose enclosing range no longer contains `cursor`.
    fn validate_cursor(&mut self, cursor: usize) {
        self.entries.retain(|e| {
            // Cursor must be strictly inside the enclosing range
            // (after opener, at or before closer).
            cursor > e.open_idx && cursor <= e.closer_idx
        });
    }
}

/// Describes what changed in the item string relative to the previous
/// real-value row.
///
/// Uses common-prefix / common-suffix to find the minimal changed span in
/// both the old and new strings.  This handles additions, removals, and
/// replacements:
///   - Pure addition:   `old_range` is empty, `new_range` covers the inserted bytes.
///   - Pure removal:    `new_range` is empty, `old_range` covers the removed bytes.
///   - Replacement:     Both ranges are non-empty.
struct ItemDiff {
    /// Byte range in the *previous* item string that was replaced/removed.
    old_range: std::ops::Range<usize>,
    /// Byte range in the *current* item string that is new/changed.
    new_range: std::ops::Range<usize>,
}

/// Compute the diff between two item strings.
/// Returns `None` when there is no previous value to compare against.
fn compute_item_diff(prev: &str, current: &str) -> ItemDiff {
    let common_prefix_bytes = prev
        .bytes()
        .zip(current.bytes())
        .take_while(|(a, b)| a == b)
        .count();

    // Snap back to a valid char boundary in both strings.
    let mut common_prefix = common_prefix_bytes;
    while common_prefix > 0
        && (!prev.is_char_boundary(common_prefix) || !current.is_char_boundary(common_prefix))
    {
        common_prefix -= 1;
    }

    // If the strings are identical, return empty ranges.
    if common_prefix == prev.len() && common_prefix == current.len() {
        return ItemDiff {
            old_range: 0..0,
            new_range: 0..0,
        };
    }

    let max_suffix_from_prev = prev.len().saturating_sub(common_prefix);
    let max_suffix_from_curr = current.len().saturating_sub(common_prefix);
    let common_suffix_bytes = prev
        .bytes()
        .rev()
        .zip(current.bytes().rev())
        .take_while(|(a, b)| a == b)
        .take(max_suffix_from_prev)
        .take(max_suffix_from_curr)
        .count();

    // Snap suffix to valid char boundaries in both strings.
    let mut common_suffix = common_suffix_bytes;
    while common_suffix > 0
        && (!prev.is_char_boundary(prev.len() - common_suffix)
            || !current.is_char_boundary(current.len() - common_suffix))
    {
        common_suffix -= 1;
    }

    ItemDiff {
        old_range: common_prefix..prev.len() - common_suffix,
        new_range: common_prefix..current.len() - common_suffix,
    }
}

struct RowData<'a> {
    pub sap: &'a SapVisualizerState,
    pub byte_idx: usize,
    pub text: String,
    /// May be different from `text.len()` since we may have escaped characters.
    pub text_len: usize,
    pub item: (Color32, Cow<'static, str>),
    /// Diff against the previous real-value row, if any.
    pub diff: Option<ItemDiff>,
}
impl RowData<'_> {
    pub fn add_char(&mut self, c: char) {
        match c {
            '\n' => self.text.push_str("\\n"),
            '\r' => self.text.push_str("\\r"),
            '\t' => self.text.push_str("\\t"),
            _ => self.text.push(c),
        }
        self.text_len += c.len_utf8();
    }
    fn render_row(self, ui: &mut egui::Ui) -> Rect {
        let RowData {
            sap,
            byte_idx,
            text,
            text_len,
            item,
            diff,
        } = self;
        let display_idx = if text_len <= 1 {
            byte_idx.to_string()
        } else {
            format!("{byte_idx}..={}", byte_idx + text_len - 1)
        };

        let num_label = ui.colored_label(Color32::GRAY, display_idx);
        let sep1 = ui.horizontal(eframe::egui::Ui::separator);
        let segment_label = ui
            .label(
                RichText::new(text)
                    .monospace()
                    .background_color(Color32::BLACK)
                    .color(Color32::WHITE),
            )
            .on_hover_ui(|ui| {
                ui.label(RichText::new(&sap.json()[..byte_idx + text_len]).monospace());
            });
        let sep2 = ui.horizontal(eframe::egui::Ui::separator);

        let (color, s) = item;
        let item_label = match diff {
            Some(ItemDiff {
                old_range,
                new_range,
            }) if !new_range.is_empty() || !old_range.is_empty() => {
                let mono = egui::FontId::monospace(14.0);
                let base_fmt = egui::TextFormat {
                    font_id: mono.clone(),
                    color,
                    ..Default::default()
                };

                if new_range.is_empty() {
                    // Pure removal: insert a red "▎" marker at the deletion point.
                    let insert_point = new_range.start; // == new_range.end
                    let mut display = s.into_owned();
                    let marker = "\u{258E}"; // ▎ thin vertical bar
                    display.insert_str(insert_point, marker);

                    let marker_fmt = egui::TextFormat {
                        font_id: mono.clone(),
                        color: Color32::from_rgb(255, 80, 80),
                        ..Default::default()
                    };
                    let mut sections = Vec::new();
                    if insert_point > 0 {
                        sections.push(LayoutSection {
                            leading_space: 0.0,
                            byte_range: 0..insert_point,
                            format: base_fmt.clone(),
                        });
                    }
                    sections.push(LayoutSection {
                        leading_space: 0.0,
                        byte_range: insert_point..insert_point + marker.len(),
                        format: marker_fmt,
                    });
                    if insert_point < display.len() - marker.len() {
                        sections.push(LayoutSection {
                            leading_space: 0.0,
                            byte_range: insert_point + marker.len()..display.len(),
                            format: base_fmt,
                        });
                    }
                    let job = egui::text::LayoutJob {
                        text: display,
                        sections,
                        wrap: TextWrapping::no_max_width(),
                        break_on_newline: false,
                        ..Default::default()
                    };
                    ui.label(job)
                } else {
                    // Addition or replacement: color the changed text and
                    // underline it.  Green for pure additions, yellow for
                    // replacements (where old content was also removed).
                    let is_replacement = !old_range.is_empty();
                    let diff_color = if is_replacement {
                        Color32::from_rgb(255, 200, 50) // yellow
                    } else {
                        Color32::from_rgb(80, 255, 80) // green
                    };
                    let highlight_fmt = egui::TextFormat {
                        font_id: mono,
                        color: diff_color,
                        underline: egui::Stroke::new(1.0_f32, diff_color),
                        ..Default::default()
                    };
                    let mut sections = Vec::new();
                    if new_range.start > 0 {
                        sections.push(LayoutSection {
                            leading_space: 0.0,
                            byte_range: 0..new_range.start,
                            format: base_fmt.clone(),
                        });
                    }
                    sections.push(LayoutSection {
                        leading_space: 0.0,
                        byte_range: new_range.clone(),
                        format: highlight_fmt,
                    });
                    if new_range.end < s.len() {
                        sections.push(LayoutSection {
                            leading_space: 0.0,
                            byte_range: new_range.end..s.len(),
                            format: base_fmt,
                        });
                    }
                    let job = egui::text::LayoutJob {
                        text: s.into_owned(),
                        sections,
                        wrap: TextWrapping::no_max_width(),
                        break_on_newline: false,
                        ..Default::default()
                    };
                    ui.label(job)
                }
            }
            _ => ui.colored_label(color, RichText::new(s).monospace()),
        };

        ui.end_row();
        num_label
            .rect
            .union(sep1.response.rect)
            .union(segment_label.rect)
            .union(sep2.response.rect)
            .union(item_label.rect)
    }
    fn make_item(
        from: &Option<Result<Option<String>, ParsingError>>,
    ) -> (Color32, Cow<'static, str>) {
        match from {
            None => (Color32::RED, Cow::Borrowed("ERROR: jsonish parse failed")),
            Some(Err(e)) => (Color32::RED, Cow::Owned(format!("ERROR: {e}"))),
            Some(Ok(None)) => (Color32::CYAN, Cow::Borrowed("== NO YIELD ==")),
            Some(Ok(Some(s))) => (Color32::WHITE, Cow::Owned(s.clone())),
        }
    }
}
