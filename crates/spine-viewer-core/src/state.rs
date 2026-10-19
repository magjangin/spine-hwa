#[derive(Debug, Clone)]
pub struct AppState {
    pub position: (f32, f32),
    pub base_position: (f32, f32), // 바운드로 계산된 기본 위치 (슬라이더 범위 기준)
    pub zoom: f32,
    pub loop_: bool,
    pub paused: bool,
    pub use_premultiply_alpha: bool,
    pub current_animation: String,
    pub current_skin: String,
    pub time: f32,
    pub timescale: f32,

    // Recording
    pub is_recording: bool,
    pub recording_fps: f32,
    pub recording_path: String,
    pub render_scale: f32,
    pub current_recording_frame: u32,
    pub saved_timescale: f32,

    // Single-frame export
    pub export_crop: bool,
    pub export_status: Option<String>,
    pub sheet_fps: f32,
    pub sheet_cols: u32,
    pub sheet_json: bool,
    pub sheet_crop: bool,
    pub sheet_scale: f32,
    pub sheet_status: Option<String>,

    // View & Display
    pub draw_border: bool,
    pub texture_size: (f32, f32),
    pub pan_offset: (f32, f32),
    pub is_panning: bool,
    pub last_mouse_pos: Option<(f32, f32)>,

    // Lists
    pub animations: Vec<String>,
    pub skins: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0),
            base_position: (0.0, 0.0),
            zoom: 1.0,
            loop_: true,
            paused: true,
            use_premultiply_alpha: true,
            current_animation: String::new(),
            current_skin: String::new(),
            time: 0.0,
            timescale: 1.0,

            is_recording: false,
            recording_fps: 30.0,
            recording_path: ".".to_string(),
            render_scale: 1.0,
            current_recording_frame: 0,
            saved_timescale: 1.0,

            export_crop: true,
            export_status: None,
            sheet_fps: 30.0,
            sheet_cols: 8,
            sheet_json: true,
            sheet_crop: true,
            sheet_scale: 1.0,
            sheet_status: None,

            draw_border: false,
            texture_size: (0.0, 0.0),
            pan_offset: (0.0, 0.0),
            is_panning: false,
            last_mouse_pos: None,

            animations: Vec::new(),
            skins: Vec::new(),
        }
    }
}
