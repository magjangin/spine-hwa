use std::path::{Path, PathBuf};
use std::sync::Arc;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2, Ui, StrokeKind};
use eframe::glow;

use crate::cli::{resolve_paths, CliArgs};
use crate::recorder::{Recorder, SheetBuilder};
use crate::renderer::GlRenderer;
use crate::spine_wrapper::SpineSkeleton;
use crate::state::AppState;
use crate::ui::render_sidebar_ui;

pub struct SpineViewerApp {
    skeleton: Option<SpineSkeleton>,
    error: Option<String>,
    fbo_tex_id: Option<egui::TextureId>,
    renderer: Option<GlRenderer>,
    state: AppState,
    max_bounds: (f32, f32, f32, f32), // x, y, w, h
    initialized: bool,
    gl_context: Option<Arc<glow::Context>>,
    last_frame_time: std::time::Instant,
}

impl SpineViewerApp {
    pub fn new(args: Option<CliArgs>) -> Result<Self, String> {
        let mut app = Self {
            skeleton: None,
            error: None,
            fbo_tex_id: None,
            renderer: None,
            state: AppState::default(),
            max_bounds: (0.0, 0.0, 100.0, 100.0),
            initialized: false,
            gl_context: None,
            last_frame_time: std::time::Instant::now(),
        };

        if let Some(args) = args {
            match SpineSkeleton::load(&args.atlas_path, &args.data_path, args.is_json) {
                Ok(skeleton) => {
                    let _ = app.set_skeleton(skeleton);
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }

        Ok(app)
    }

    fn sk(&self) -> &SpineSkeleton {
        self.skeleton.as_ref().expect("skeleton not loaded")
    }

    /// 새 스켈레톤으로 교체한다. 실패하면 이전 상태를 그대로 복원한다.
    fn set_skeleton(&mut self, skeleton: SpineSkeleton) -> Result<Option<SpineSkeleton>, String> {
        let anims = skeleton.get_animations();
        if anims.is_empty() {
            return Err("스켈레톤 데이터에 애니메이션이 없습니다.".to_string());
        }
        let skins = skeleton.get_skins();

        let prev_state = self.state.clone();
        let prev_bounds = self.max_bounds;
        let prev = self.skeleton.replace(skeleton);

        self.state.current_animation = anims[0].clone();
        self.state.animations = anims;
        self.state.current_skin = skins.first().cloned().unwrap_or_default();
        self.state.skins = skins;
        self.state.loop_ = true;
        self.state.paused = true; // 기본 정지 상태로 로드
        self.state.time = 0.0;
        self.state.zoom = 1.0;
        self.state.pan_offset = (0.0, 0.0);

        // 최대 바운드 계산
        if let Err(e) = self.update_render_texture_bounds() {
            self.skeleton = prev;
            self.state = prev_state;
            self.max_bounds = prev_bounds;
            return Err(e);
        }
        Ok(prev)
    }

    /// 선택/드롭된 파일로 모델을 연다. 실패 시 기존 모델을 유지하고 오류를 표시한다.
    fn open_files(&mut self, paths: Vec<PathBuf>) {
        if self.state.is_recording {
            return;
        }
        let result = resolve_paths(&paths).and_then(|a| {
            let skeleton = SpineSkeleton::load(&a.atlas_path, &a.data_path, a.is_json)?;
            let prev_state = self.state.clone();
            let prev_bounds = self.max_bounds;
            let prev = self.set_skeleton(skeleton)?;
            if let (Some(gl), true) = (self.gl_context.clone(), self.renderer.is_some()) {
                if let Err(e) = self.setup_textures(&gl) {
                    self.skeleton = prev;
                    self.state = prev_state;
                    self.max_bounds = prev_bounds;
                    if self.skeleton.is_some() {
                        let _ = self.setup_textures(&gl);
                    }
                    return Err(e);
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            self.error = Some(format!("스켈레톤 로드 실패:\n{}", e));
        } else {
            self.error = None;
        }
    }

    fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Spine 스켈레톤 파일 열기")
            .add_filter("Spine 에셋 (*.skel, *.json, *.atlas)", &["skel", "json", "atlas"])
            .pick_files()
        {
            self.open_files(files);
        }
    }

    fn show_error(&mut self, ctx: &egui::Context) {
        if let Some(msg) = self.error.clone() {
            let mut close_error = false;
            egui::Window::new("오류")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&msg);
                    ui.add_space(8.0);
                    if ui.button("확인").clicked() {
                        close_error = true;
                    }
                });
            if close_error {
                self.error = None;
            }
        }
    }

    fn update_render_texture_bounds(&mut self) -> Result<(), String> {
        let anim_name = self.state.current_animation.clone();
        let bounds = self.sk().compute_max_bounds(&anim_name, 0.016)?;

        let w = (bounds.width * self.state.render_scale).max(1.0);
        let h = (bounds.height * self.state.render_scale).max(1.0);
        self.max_bounds = (bounds.x, bounds.y, w, h);

        self.state.texture_size = (w, h);

        // Skeleton position = -(max bounds top-left)
        self.state.position = (-bounds.x, -bounds.y);
        self.state.base_position = self.state.position;
        self.sk().set_position(self.state.position.0, self.state.position.1);

        // Restore animation on track 0 with current loop setting
        self.sk().set_animation(&anim_name, self.state.loop_);
        self.sk().set_time(0.0);

        Ok(())
    }

    /// 현재 애니메이션 전 구간을 `sheet_fps` 간격으로 렌더해 격자 시트 PNG(+json)로 저장한다.
    fn export_sheet(&mut self) -> Result<String, String> {
        let dir = Recorder::validate_path(&self.state.recording_path)?;
        let gl = self.gl_context.clone().ok_or("GL 컨텍스트가 없습니다.")?;
        let renderer = self.renderer.as_ref().ok_or("렌더러가 초기화되지 않았습니다.")?;
        let (fbo_w, fbo_h) = (renderer.fbo_width as u32, renderer.fbo_height as u32);
        if fbo_w == 0 || fbo_h == 0 {
            return Err("렌더 영역이 비어 있습니다.".to_string());
        }

        let fps = self.state.sheet_fps.clamp(1.0, 60.0);
        let duration = self.sk().get_duration().max(0.001);
        let count = ((duration * fps).ceil() as u32).max(1);
        let pma = self.state.use_premultiply_alpha;
        let prev_time = self.state.time;

        let render_frame = |i: u32| -> Vec<u8> {
            self.sk().set_time(i as f32 / fps);
            self.sk().update(0.0);
            let mesh = self.sk().generate_mesh(pma);
            unsafe {
                renderer.render_to_fbo(&gl, mesh.vertices, mesh.indices, mesh.batches, pma);
                renderer.read_fbo_pixels(&gl)
            }
        };

        // 1패스: 전 프레임의 실제 내용 합집합 영역을 구해 셀 크기로 쓴다 (최대 바운드보다 훨씬 작을 수 있음).
        let rect = if self.state.sheet_crop {
            let mut union: Option<(u32, u32, u32, u32)> = None;
            for i in 0..count {
                if let Some(b) = Recorder::content_bounds(fbo_w, fbo_h, &render_frame(i)) {
                    union = Some(match union {
                        None => b,
                        Some(u) => (u.0.min(b.0), u.1.min(b.1), u.2.max(b.2), u.3.max(b.3)),
                    });
                }
            }
            union.ok_or("애니메이션 전 구간에 보이는 픽셀이 없습니다.")?
        } else {
            (0, 0, fbo_w - 1, fbo_h - 1)
        };
        let (src_w, src_h) = (rect.2 - rect.0 + 1, rect.3 - rect.1 + 1);

        let cols = self.state.sheet_cols.clamp(1, count);
        let rows = (count + cols - 1) / cols;

        // 사용자 배율, 그리고 한계(한 변/총 픽셀)를 넘으면 자동으로 더 줄인다.
        let full_w = (src_w * cols) as f32;
        let full_h = (src_h * rows) as f32;
        let fit_side = (SheetBuilder::MAX_SIDE as f32 / full_w).min(SheetBuilder::MAX_SIDE as f32 / full_h);
        let fit_area = (SheetBuilder::MAX_PIXELS as f32 / (full_w * full_h)).sqrt();
        let scale = self.state.sheet_scale.clamp(0.01, 1.0).min(fit_side).min(fit_area);
        let cell_w = ((src_w as f32 * scale).round() as u32).max(1);
        let cell_h = ((src_h as f32 * scale).round() as u32).max(1);
        let mut sheet = SheetBuilder::new(cell_w, cell_h, cols, rows)?;

        // 2패스: 각 프레임을 합집합 영역으로 잘라(필요하면 축소해) 시트에 배치
        for i in 0..count {
            let pixels = render_frame(i);
            let cropped = Recorder::crop_rect(fbo_w, &pixels, rect);
            if cell_w == src_w && cell_h == src_h {
                sheet.blit(i, &cropped);
            } else {
                let img = image::RgbaImage::from_raw(src_w, src_h, cropped).ok_or("프레임 버퍼 크기 오류")?;
                let small = image::imageops::resize(&img, cell_w, cell_h, image::imageops::FilterType::Lanczos3);
                sheet.blit(i, small.as_raw());
            }
        }
        // 원래 시점으로 복원
        self.sk().set_time(prev_time);
        self.sk().update(0.0);

        let stem = format!("{}_sheet", Recorder::sanitize(&self.state.current_animation));
        let png_path = dir.join(format!("{}.png", stem));
        sheet.save(&png_path)?;

        if self.state.sheet_json {
            let json = format!(
                "{{\n  \"image\": \"{}.png\",\n  \"frameWidth\": {},\n  \"frameHeight\": {},\n  \"offsetX\": {},\n  \"offsetY\": {},\n  \"scale\": {},\n  \"columns\": {},\n  \"rows\": {},\n  \"frameCount\": {},\n  \"fps\": {},\n  \"duration\": {}\n}}\n",
                stem, cell_w, cell_h, rect.0, rect.1, scale, cols, rows, count, fps, duration
            );
            std::fs::write(dir.join(format!("{}.json", stem)), json)
                .map_err(|e| format!("json 저장 실패: {}", e))?;
        }
        Ok(format!(
            "저장됨: {} ({}프레임, 셀 {}x{}, 시트 {}x{}, 배율 {:.0}%)",
            png_path.display(),
            count,
            cell_w,
            cell_h,
            sheet.width(),
            sheet.height(),
            scale * 100.0
        ))
    }

    fn init_gl(&mut self, gl: &glow::Context) -> Result<(), String> {
        let renderer = unsafe { GlRenderer::new(gl)? };
        self.renderer = Some(renderer);
        self.initialized = true;

        if self.skeleton.is_some() {
            self.setup_textures(gl)?;
        }
        Ok(())
    }

    /// 현재 스켈레톤의 아틀라스 텍스처를 올리고 FBO 크기를 맞춘다.
    fn setup_textures(&mut self, gl: &glow::Context) -> Result<(), String> {
        let page_count = self.sk().get_page_count();
        let tex_paths: Vec<String> = (0..page_count).map(|i| self.sk().get_page_path(i)).collect();

        let w = self.max_bounds.2 as i32;
        let h = self.max_bounds.3 as i32;
        let renderer = self.renderer.as_mut().ok_or("renderer not initialized")?;
        unsafe { renderer.clear_textures(gl) };
        let sizes = unsafe { renderer.load_textures(gl, &tex_paths)? };
        unsafe { renderer.resize_render_texture(gl, w, h)? };
        for (i, (tw, th)) in sizes.iter().enumerate() {
            self.sk().set_page_texture_size(i as i32, *tw, *th);
        }
        Ok(())
    }
}

impl eframe::App for SpineViewerApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // 0. 최상단 메뉴 바
        egui::Panel::top("top_menu_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("파일", |ui| {
                    if ui.button("스켈레톤 열기...").clicked() {
                        self.pick_files();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("종료").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("보기", |ui| {
                    if ui.button("화면 위치/배율 리셋").clicked() {
                        self.state.zoom = 1.0;
                        self.state.pan_offset = (0.0, 0.0);
                        ui.close();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Spine 3.8.99 Viewer");
                });
            });
        });

        // GL 렌더러 1회 초기화
        if !self.initialized {
            if let Some(gl) = frame.gl() {
                self.gl_context = Some(gl.clone());
                if let Err(e) = self.init_gl(&gl) {
                    eprintln!("Failed to initialize GL renderer: {}", e);
                    self.error = Some(e);
                }
            }
        }

        let now = std::time::Instant::now();
        let dt = (now - self.last_frame_time).as_secs_f32().min(0.1);
        self.last_frame_time = now;

        // 드래그 앤 드롭 파일 로드
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect()
        });
        if !dropped.is_empty() {
            self.open_files(dropped);
        }

        // 스켈레톤이 아직 로드되지 않은 경우: 환영 안내 및 파일 열기 대시보드
        if self.skeleton.is_none() {
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 36));

            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.3);
                ui.heading("Spine 3.8.99 뷰어 (Rust)");
                ui.add_space(10.0);
                ui.label("스켈레톤 파일(.skel, .json) 또는 아틀라스(.atlas)를 열어주세요.");
                ui.label("(뮤즈대쉬 등 Spine 3.8.99 바이너리 및 JSON 완벽 지원)");
                ui.add_space(16.0);

                let open_btn = egui::Button::new("스켈레톤 파일 열기...").min_size(egui::vec2(220.0, 36.0));
                if ui.add(open_btn).clicked() {
                    self.pick_files();
                }

                ui.add_space(12.0);
                ui.label("팁: 탐색기에서 파일을 이 창 안으로 직접 드래그 앤 드롭해도 열립니다.");
            });

            self.show_error(&ctx);
            return;
        }

        // 1. 좌측 사이드바 패널 (모든 조작 UI 통합 배치 -> 중앙 캐릭터를 절대 가리지 않음)
        let duration = self.sk().get_duration();
        let mut ui_action = crate::ui::UiAction::default();

        egui::Panel::left("left_control_sidebar")
            .resizable(true)
            .default_size(300.0)
            .min_size(240.0)
            .max_size(480.0)
            .show(ui, |ui| {
                ui_action = render_sidebar_ui(ui, &mut self.state, duration);
            });

        self.show_error(&ctx);

        // 2. 마우스 입력 (캔버스 영역 위에서만 줌 & 팬 적용)
        let is_pointer_over_egui = ctx.is_pointer_over_egui();

        ctx.input(|i| {
            // 마우스 휠 줌 (사이드바 밖에서만)
            if !is_pointer_over_egui {
                let scroll_delta = i.smooth_scroll_delta.y;
                if scroll_delta != 0.0 {
                    let delta = if scroll_delta > 0.0 { 1.0 } else { -1.0 };
                    self.state.zoom = (self.state.zoom + delta * 0.02).clamp(0.0, 3.0);
                }
            }

            // 좌클릭 드래그 팬 (사이드바 밖에서만)
            if i.pointer.primary_down() {
                if !is_pointer_over_egui {
                    if let Some(pos) = i.pointer.interact_pos() {
                        if let Some(last_pos) = self.state.last_mouse_pos {
                            let delta_x = pos.x - last_pos.0;
                            let delta_y = pos.y - last_pos.1;
                            self.state.pan_offset.0 += delta_x;
                            self.state.pan_offset.1 += delta_y;
                        }
                        self.state.last_mouse_pos = Some((pos.x, pos.y));
                        self.state.is_panning = true;
                    }
                }
            } else {
                self.state.is_panning = false;
                self.state.last_mouse_pos = None;
            }
        });

        // 3. UI 액션 반영
        if ui_action.open_requested {
            self.pick_files();
        }

        if ui_action.reset_view {
            self.state.zoom = 1.0;
            self.state.pan_offset = (0.0, 0.0);
        }

        // 애니메이션 변경
        if let Some(new_anim) = ui_action.animation_changed {
            let current_loop = self.sk().get_loop();
            if self.sk().set_animation(&new_anim, current_loop) {
                let _ = self.update_render_texture_bounds();
                if let (Some(gl), Some(renderer)) = (&self.gl_context, &mut self.renderer) {
                    let w = self.max_bounds.2 as i32;
                    let h = self.max_bounds.3 as i32;
                    unsafe {
                        let _ = renderer.resize_render_texture(gl, w, h);
                    }
                }
            }
        }

        // 스킨 변경
        if let Some(new_skin) = ui_action.skin_changed {
            if self.sk().set_skin(&new_skin) {
                let _ = self.update_render_texture_bounds();
            }
        }

        // 수동 시간 조작
        if let Some(new_time) = ui_action.time_changed {
            self.sk().set_time(new_time);
            self.sk().update(0.0);
        }

        // 재생 속도 및 루프 동기화
        if !self.state.is_recording {
            self.sk().set_timescale(self.state.timescale);
            self.sk().set_loop(self.state.loop_);
        }

        // 스켈레톤 위치 동기화
        self.sk().set_position(self.state.position.0, self.state.position.1);

        // 녹화 시작 요청
        if ui_action.start_recording && !self.state.is_recording {
            match Recorder::validate_path(&self.state.recording_path) {
                Ok(valid_dir) => {
                    self.state.recording_path = valid_dir.to_string_lossy().into_owned();
                    self.state.is_recording = true;
                    self.state.current_recording_frame = 0;
                    self.state.saved_timescale = self.sk().get_timescale();

                    if (self.state.render_scale - 1.0).abs() > 0.001 {
                        self.sk().set_scale(self.state.render_scale, self.state.render_scale);
                        let _ = self.update_render_texture_bounds();
                        if let (Some(gl), Some(renderer)) = (&self.gl_context, &mut self.renderer) {
                            let w = self.max_bounds.2 as i32;
                            let h = self.max_bounds.3 as i32;
                            unsafe {
                                let _ = renderer.resize_render_texture(gl, w, h);
                            }
                        }
                    }

                    self.sk().set_time(0.0);
                    self.sk().set_timescale(1.0);
                    self.sk().set_loop(false);
                    self.sk().update(0.0);
                }
                Err(e) => {
                    self.state.is_recording = false;
                    self.error = Some(format!("녹화 경로 오류: {}", e));
                }
            }
        }

        // 녹화 중지 요청
        if ui_action.stop_recording && self.state.is_recording {
            self.state.is_recording = false;
            if (self.state.render_scale - 1.0).abs() > 0.001 {
                self.sk().set_scale(1.0, 1.0);
                let _ = self.update_render_texture_bounds();
                if let (Some(gl), Some(renderer)) = (&self.gl_context, &mut self.renderer) {
                    let w = self.max_bounds.2 as i32;
                    let h = self.max_bounds.3 as i32;
                    unsafe {
                        let _ = renderer.resize_render_texture(gl, w, h);
                    }
                }
            }
            self.sk().set_timescale(self.state.saved_timescale);
        }

        // 4. 업데이트 및 녹화 루프
        if !self.state.is_recording {
            if !self.state.paused {
                self.sk().update(dt);
                self.state.time = self.sk().get_time();
            } else {
                // 일시 정지 상태: 애니메이션 시간은 흐르지 않고 포즈 유지
                self.sk().update(0.0);
            }
        } else {
            // 녹화 프레임 FBO 렌더 및 PNG 저장
            let pma = self.state.use_premultiply_alpha;
            let mesh = self.sk().generate_mesh(pma);
            if let (Some(gl), Some(renderer)) = (&self.gl_context, &self.renderer) {
                unsafe {
                    renderer.render_to_fbo(gl, mesh.vertices, mesh.indices, mesh.batches, pma);
                    let pixels = renderer.read_fbo_pixels(gl);
                    let _ = Recorder::save_frame(
                        Path::new(&self.state.recording_path),
                        &self.state.current_animation,
                        self.state.current_recording_frame,
                        renderer.fbo_width as u32,
                        renderer.fbo_height as u32,
                        &pixels,
                    );
                }
            }

            self.state.current_recording_frame += 1;
            let step_dt = 1.0 / self.state.recording_fps.max(0.01);
            self.sk().update(step_dt);
            self.state.time = self.sk().get_time();

            // 녹화 종료 판정 (t >= duration)
            let track_dur = self.sk().get_duration();
            if self.state.time >= track_dur {
                self.state.is_recording = false;
                if (self.state.render_scale - 1.0).abs() > 0.001 {
                    self.sk().set_scale(1.0, 1.0);
                    let _ = self.update_render_texture_bounds();
                    if let (Some(gl), Some(renderer)) = (&self.gl_context, &mut self.renderer) {
                        let w = self.max_bounds.2 as i32;
                        let h = self.max_bounds.3 as i32;
                        unsafe {
                            let _ = renderer.resize_render_texture(gl, w, h);
                        }
                    }
                }
                self.sk().set_timescale(self.state.saved_timescale);
            }
        }

        // 5. 일반 화면 표시용 FBO 렌더링
        let pma = self.state.use_premultiply_alpha;
        let mesh = self.sk().generate_mesh(pma);
        if let (Some(gl), Some(renderer)) = (&self.gl_context, &self.renderer) {
            unsafe {
                renderer.render_to_fbo(gl, mesh.vertices, mesh.indices, mesh.batches, pma);
            }
        }

        // 현재 프레임 1장 저장 (방금 렌더한 FBO를 그대로 읽는다)
        if ui_action.save_frame && !self.state.is_recording {
            let result = Recorder::validate_path(&self.state.recording_path).and_then(|dir| {
                let renderer = self.renderer.as_ref().ok_or("렌더러가 초기화되지 않았습니다.")?;
                let gl = self.gl_context.as_ref().ok_or("GL 컨텍스트가 없습니다.")?;
                let pixels = unsafe { renderer.read_fbo_pixels(gl) };
                Recorder::save_snapshot(
                    &dir,
                    &self.state.current_animation,
                    self.state.time,
                    self.state.export_crop,
                    renderer.fbo_width as u32,
                    renderer.fbo_height as u32,
                    &pixels,
                )
            });
            self.state.export_status = Some(match result {
                Ok(path) => format!("저장됨: {}", path.display()),
                Err(e) => format!("저장 실패: {}", e),
            });
        }

        if ui_action.save_sheet && !self.state.is_recording {
            self.state.sheet_status = Some(match self.export_sheet() {
                Ok(msg) => msg,
                Err(e) => format!("저장 실패: {}", e),
            });
            // 시트 렌더가 FBO에 마지막 프레임을 남기므로 다음 프레임에 원래 포즈로 다시 그린다.
            ctx.request_repaint();
        }

        // 6. 메인 뷰포트 영역 (사이드바 우측 전체 캔버스 -> 캐릭터가 훤히 보임!)
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter();

        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 36));

        if self.fbo_tex_id.is_none() {
            if let Some(renderer) = &self.renderer {
                self.fbo_tex_id = Some(frame.register_native_glow_texture(renderer.fbo_texture()));
            }
        }
        if let (Some(renderer), Some(tex_id)) = (&self.renderer, self.fbo_tex_id) {
            let tw = renderer.fbo_width as f32 * self.state.zoom;
            let th = renderer.fbo_height as f32 * self.state.zoom;

            let center_x = rect.min.x + (rect.width() - tw) * 0.5 + self.state.pan_offset.0;
            let center_y = rect.min.y + (rect.height() - th) * 0.5 + self.state.pan_offset.1;

            let sprite_rect = Rect::from_min_size(
                Pos2::new(center_x, center_y),
                Vec2::new(tw, th),
            );

            let uv_rect = Rect::from_min_max(Pos2::new(0.0, 1.0), Pos2::new(1.0, 0.0));
            painter.image(tex_id, sprite_rect, uv_rect, Color32::WHITE);

            // 외곽 테두리
            if self.state.draw_border {
                let stroke_thickness = 2.0;
                painter.rect_stroke(
                    sprite_rect,
                    0.0,
                    Stroke::new(stroke_thickness, Color32::from_rgb(255, 60, 60)),
                    StrokeKind::Outside,
                );
            }
        }

        // 애니메이션 연속 재생 요청
        if !self.state.paused || self.state.is_recording {
            ctx.request_repaint();
        }
    }
}
