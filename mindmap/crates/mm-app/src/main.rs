use std::path::PathBuf;

fn main() -> eframe::Result {
    let arg: Option<PathBuf> = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mindmap")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("Mindmap", options, Box::new(move |cc| Ok(Box::new(mm_app::app::MindmapApp::new(cc, arg.clone())))))
}
