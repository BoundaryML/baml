use ::bex_sap::deserializer::coercer::ParsingError;
use ::eframe::egui::{
    self, Color32, Rect, RichText, TextBuffer,
    text::{LayoutSection, TextWrapping},
};
use ::std::borrow::Cow;
use tools_sap_visualizer::SapVisualizerState;

struct SapVisualizer {
    sap: SapVisualizerState<&'static str>,
    text_highlight: Option<(usize, usize)>,
}

impl SapVisualizer {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let db = bex_sap::baml_db! {
            enum Degree {
                HighSchool,
                Associate,
                Bachelor,
                Master,
                Doctorate @alias("Doctorate") @alias("PhD"),
            }

            class Education {
                school: (string | null) @class_in_progress_field_missing(null) @class_completed_field_missing(never),
                degree: (Degree @in_progress(never) | string | null) @parse_as(Degree | string) @class_in_progress_field_missing(null) @class_completed_field_missing(never),
                // null if not completed yet
                year: (int | null) @in_progress(never) @class_in_progress_field_missing(null) @class_completed_field_missing(null),
            }

            class Resume {
                name: (string | null) @parse_as(string) @class_in_progress_field_missing(null) @class_completed_field_missing(never),
                email: (string | null) @parse_as(string) @class_in_progress_field_missing(null) @class_completed_field_missing(never),
                phone: (string | null) @parse_as(string) @class_in_progress_field_missing(null) @class_completed_field_missing(never),
                experience: [int @in_progress(never)] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
                education: [Education] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
                skills: [string] @class_in_progress_field_missing([]) @class_completed_field_missing([]),
            }
        };
        let ty = bex_sap::baml_tyannotated!(Resume);
        let sap = SapVisualizerState::new(String::default(), db, ty);
        Self {
            sap,
            text_highlight: None,
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
        eframe::egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.take_available_width();
            ui.vertical(|ui| {
                ui.take_available_width();
                ui.label("JSON:");
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
                egui::TextEdit::multiline(&mut self.sap)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .hint_text("Put some JSON here")
                    .layouter(&mut layouter)
                    .show(ui);
            });
        });
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.take_available_space();
            ui.vertical(|ui| {
                ui.take_available_space();
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("output").show(ui, |ui| {
                        let mut prev = RowData {
                            sap: &self.sap,
                            byte_idx: 0,
                            text: "".to_string(),
                            text_len: 0,
                            item: RowData::make_item(self.sap.iter().next().unwrap_or_default()),
                        };
                        let mut pointer = None;
                        ctx.input(|i| pointer = i.pointer.hover_pos());
                        self.text_highlight = None; // reset highlight, may be updated by `render_row`
                        for ((byte_idx, c), item) in std::iter::zip(
                            self.sap
                                .json()
                                .char_indices()
                                .chain(std::iter::once((self.sap.json().len(), '\0'))),
                            self.sap.iter().skip(1),
                        ) {
                            let item = match item {
                                // jsonish error
                                None => {
                                    (Color32::RED, Cow::Borrowed("ERROR: jsonish parse failed"))
                                }
                                Some(Err(e)) => (Color32::RED, Cow::Owned(format!("ERROR: {e}"))),
                                Some(Ok(None)) => (Color32::CYAN, Cow::Borrowed("== NO YIELD ==")),
                                Some(Ok(Some(s))) => (Color32::WHITE, Cow::Owned(s)),
                            };
                            if prev.item == item {
                                prev.add_char(c);
                            } else {
                                let would_be_highlight =
                                    (prev.byte_idx, prev.byte_idx + prev.text_len);
                                let row_rect = prev.render_row(ui);
                                if pointer.is_some_and(|pointer| row_rect.contains(pointer)) {
                                    self.text_highlight = Some(would_be_highlight);
                                }
                                prev = RowData {
                                    sap: &self.sap,
                                    byte_idx,
                                    text: c.to_string(),
                                    text_len: c.len_utf8(),
                                    item,
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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "SAP Visualizer",
        options,
        Box::new(|cc| Ok(Box::new(SapVisualizer::new(cc)))),
    )
}

struct RowData<'a> {
    pub sap: &'a SapVisualizerState<&'static str>,
    pub byte_idx: usize,
    pub text: String,
    /// May be different from `text.len()` since we may have escaped characters.
    pub text_len: usize,
    pub item: (Color32, Cow<'static, str>),
}
impl<'a> RowData<'a> {
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
        } = self;
        let display_idx = if text_len <= 1 {
            byte_idx.to_string()
        } else {
            format!("{byte_idx}..={}", byte_idx + text_len - 1)
        };

        let num_label = ui.colored_label(Color32::GRAY, display_idx);
        let sep1 = ui.horizontal(|ui| ui.separator());
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
        let sep2 = ui.horizontal(|ui| ui.separator());
        let (color, s) = item;
        let item_label = ui.colored_label(color, RichText::new(s).monospace());
        ui.end_row();
        num_label
            .rect
            .union(sep1.response.rect)
            .union(segment_label.rect)
            .union(sep2.response.rect)
            .union(item_label.rect)
    }
    fn make_item(
        from: Option<Result<Option<String>, ParsingError>>,
    ) -> (Color32, Cow<'static, str>) {
        match from {
            None => (Color32::RED, Cow::Borrowed("ERROR: jsonish parse failed")),
            Some(Err(e)) => (Color32::RED, Cow::Owned(format!("ERROR: {e}"))),
            Some(Ok(None)) => (Color32::CYAN, Cow::Borrowed("== NO YIELD ==")),
            Some(Ok(Some(s))) => (Color32::WHITE, Cow::Owned(s)),
        }
    }
}
