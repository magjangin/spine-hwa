pub mod cli;
pub mod spine_wrapper;
pub mod renderer;
pub mod state;
pub mod recorder;
pub mod export;
pub mod headless;
pub mod ui;
pub mod app;

use cli::CliArgs;
use app::SpineViewerApp;

pub fn setup_korean_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Windows 표준 맑은 고딕 (Malgun Gothic) 시도
    let font_paths = [
        "C:\\Windows\\Fonts\\malgun.ttf",
        "C:\\Windows\\Fonts\\malgunbd.ttf",
        "C:\\Windows\\Fonts\\gulim.ttc",
    ];

    for path in &font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "korean_font".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "korean_font".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("korean_font".to_owned());
            break;
        }
    }

    ctx.set_fonts(fonts);
}

pub fn run() {
    let args_vec: Vec<String> = std::env::args().collect();
    let has_explicit_args = args_vec.len() > 1;

    let args: Option<CliArgs> = match CliArgs::parse() {
        Ok(args) => args,
        Err(err) => {
            if has_explicit_args {
                eprintln!("{}", err);
                std::process::exit(1);
            }
            None
        }
    };

    let app = match SpineViewerApp::new(args) {
        Ok(app) => app,
        Err(err) => {
            if has_explicit_args {
                eprint!("스켈레톤 로드 실패: {}", err);
                std::process::exit(1);
            }
            SpineViewerApp::new(None).unwrap()
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Spine 3.8.99 Viewer"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "Spine 3.8.99 Viewer",
        native_options,
        Box::new(move |cc| {
            setup_korean_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
}
