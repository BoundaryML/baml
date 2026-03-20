#[cfg(not(target_arch = "wasm32"))]
use ::tools_sap_visualizer::ui::SapVisualizer;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
fn main() -> eframe::Result {
    unimplemented!("WASM not supported in executable form.");
}
