fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 520.0])
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Folio",
        options,
        Box::new(|cc| Ok(Box::new(folio::app::App::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    let png = include_bytes!("../build/icon-128.png");
    let img = image::load_from_memory(png).expect("icon").to_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        width: w,
        height: h,
        rgba: img.into_raw(),
    }
}
