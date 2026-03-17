use ::std::borrow::Cow;

use ::bex_sap::deserializer::coercer::ParsingError;
use ::eframe::egui::{self, Color32, RichText};
use tools_sap_visualizer::SapVisualizerState;

struct SapVisualizer {
    sap: SapVisualizerState<&'static str>,
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
        Self { sap }
    }
}

impl eframe::App for SapVisualizer {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        eframe::egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.take_available_width();
            ui.vertical(|ui| {
                ui.take_available_width();
                ui.label("JSON:");
                egui::TextEdit::multiline(&mut self.sap)
                    .code_editor()
                    .desired_width(f32::INFINITY)
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
                            item: RowData::make_item(self.sap.iter().next().unwrap_or_default()),
                        };
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
                                prev.render_row(ui);
                                prev = RowData {
                                    sap: &self.sap,
                                    byte_idx,
                                    text: c.to_string(),
                                    item,
                                };
                            }
                        }
                        prev.render_row(ui);
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
    }
    fn render_row(self, ui: &mut egui::Ui) {
        let RowData {
            sap,
            byte_idx,
            text,
            item,
        } = self;
        let text_len = text.len();
        let display_idx = if text_len <= 1 {
            byte_idx.to_string()
        } else {
            format!("{byte_idx}..={}", byte_idx + text_len - 1)
        };
        ui.colored_label(Color32::GRAY, display_idx);
        ui.horizontal(|ui| ui.separator());
        ui.label(
            RichText::new(text)
                .monospace()
                .background_color(Color32::BLACK)
                .color(Color32::WHITE),
        )
        .on_hover_ui(|ui| {
            ui.label(RichText::new(&sap.json()[..byte_idx + text_len]));
        });
        ui.horizontal(|ui| ui.separator());
        let (color, s) = item;
        ui.colored_label(color, RichText::new(s).monospace());
        ui.end_row();
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
