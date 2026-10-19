use egui::{Ui, Slider, DragValue, ProgressBar, Button, ComboBox, ScrollArea};
use crate::state::AppState;

pub struct UiAction {
    pub animation_changed: Option<String>,
    pub skin_changed: Option<String>,
    pub start_recording: bool,
    pub stop_recording: bool,
    pub time_changed: Option<f32>,
    pub open_requested: bool,
    pub reset_view: bool,
    pub save_frame: bool,
}

impl Default for UiAction {
    fn default() -> Self {
        Self {
            animation_changed: None,
            skin_changed: None,
            start_recording: false,
            stop_recording: false,
            time_changed: None,
            open_requested: false,
            reset_view: false,
            save_frame: false,
        }
    }
}

pub fn render_sidebar_ui(ui: &mut Ui, state: &mut AppState, duration: f32) -> UiAction {
    let mut action = UiAction::default();

    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(4.0);

        // 1. 파일 열기 버튼
        if !state.is_recording {
            let open_btn = Button::new("스켈레톤 파일 열기...").min_size(egui::vec2(ui.available_width(), 32.0));
            if ui.add(open_btn).clicked() {
                action.open_requested = true;
            }
            ui.add_space(8.0);
            ui.separator();
        }

        // 2. 애니메이션 제어 섹션
        ui.heading("애니메이션 제어");
        ui.add_space(4.0);

        if !state.is_recording {
            // 재생 / 정지 버튼 모음
            ui.horizontal_wrapped(|ui| {
                let play_pause_text = if state.paused { "재생" } else { "일시정지" };
                if ui.button(play_pause_text).clicked() {
                    state.paused = !state.paused;
                }
                if ui.button("정지").clicked() {
                    state.paused = true;
                    state.time = 0.0;
                    action.time_changed = Some(0.0);
                }
                if ui.button("처음으로").clicked() {
                    state.time = 0.0;
                    action.time_changed = Some(0.0);
                }
                if ui.button("-1프레임").clicked() {
                    state.paused = true;
                    let new_t = (state.time - 1.0 / 60.0).max(0.0);
                    state.time = new_t;
                    action.time_changed = Some(new_t);
                }
                if ui.button("+1프레임").clicked() {
                    state.paused = true;
                    let max_dur = duration.max(0.001);
                    let new_t = (state.time + 1.0 / 60.0).min(max_dur);
                    state.time = new_t;
                    action.time_changed = Some(new_t);
                }
            });

            ui.add_space(6.0);

            // 시간 슬라이더
            let max_dur = duration.max(0.001);
            let prev_time = state.time;
            ui.horizontal(|ui| {
                let slider_resp = ui.add(
                    Slider::new(&mut state.time, 0.0..=max_dur)
                        .text("시간")
                        .clamping(egui::SliderClamping::Always)
                        .max_decimals(2),
                );
                if slider_resp.changed() || state.time != prev_time {
                    action.time_changed = Some(state.time);
                }
            });
            ui.label(format!("현재: {:.2}s / 전체: {:.2}s", state.time, duration));

            // 재생 속도
            ui.horizontal(|ui| {
                ui.label("속도");
                ui.add(
                    DragValue::new(&mut state.timescale)
                        .speed(0.05)
                        .range(0.0..=3.0)
                        .max_decimals(2),
                );
                if ui.button("1.0x").clicked() {
                    state.timescale = 1.0;
                }
                if ui.button("0.5x").clicked() {
                    state.timescale = 0.5;
                }
            });

            ui.checkbox(&mut state.loop_, "반복 재생 (Loop)");

            ui.add_space(8.0);
            ui.separator();

            // 3. 모델 및 뷰 설정 섹션
            ui.heading("모델 및 뷰 설정");
            ui.add_space(4.0);

            // 애니메이션 선택
            ui.label("애니메이션");
            ComboBox::from_id_salt("anim_combo")
                .selected_text(&state.current_animation)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for anim in &state.animations {
                        let is_selected = state.current_animation == *anim;
                        if ui.selectable_value(&mut state.current_animation, anim.clone(), anim).clicked() {
                            if !is_selected {
                                action.animation_changed = Some(anim.clone());
                            }
                        }
                    }
                });

            // 스킨 선택
            ui.add_space(4.0);
            ui.label("스킨");
            ComboBox::from_id_salt("skin_combo")
                .selected_text(&state.current_skin)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for skin in &state.skins {
                        let is_selected = state.current_skin == *skin;
                        if ui.selectable_value(&mut state.current_skin, skin.clone(), skin).clicked() {
                            if !is_selected {
                                action.skin_changed = Some(skin.clone());
                            }
                        }
                    }
                });

            ui.add_space(6.0);
            ui.checkbox(&mut state.draw_border, "외곽 테두리 표시 (Border)");
            ui.checkbox(&mut state.use_premultiply_alpha, "PMA (알파 미리 곱하기)");

            ui.add_space(4.0);
            let tex_w = state.texture_size.0;
            let tex_h = state.texture_size.1;
            let (bx, by) = state.base_position;
            let min_x = (-0.5 * tex_w).min(bx - 0.5 * tex_w);
            let max_x = (1.5 * tex_w).max(bx + 1.5 * tex_w);
            let min_y = (-0.5 * tex_h).min(by - 0.5 * tex_h);
            let max_y = (1.5 * tex_h).max(by + 1.5 * tex_h);

            ui.add(
                Slider::new(&mut state.position.0, min_x..=max_x)
                    .text("위치 X")
                    .clamping(egui::SliderClamping::Always),
            );
            ui.add(
                Slider::new(&mut state.position.1, min_y..=max_y)
                    .text("위치 Y")
                    .clamping(egui::SliderClamping::Always),
            );

            ui.horizontal(|ui| {
                ui.label("배율(줌)");
                ui.add(
                    DragValue::new(&mut state.zoom)
                        .speed(0.05)
                        .range(0.0..=3.0)
                        .max_decimals(2),
                );
                if ui.button("100%").clicked() {
                    state.zoom = 1.0;
                }
                if ui.button("중앙 정렬").clicked() {
                    action.reset_view = true;
                }
            });

            ui.add_space(8.0);
            ui.separator();

            // 3-1. 현재 프레임 저장 섹션 (저장 폴더는 아래 녹화 섹션과 공유)
            ui.heading("현재 프레임 저장");
            ui.add_space(4.0);
            ui.checkbox(&mut state.export_crop, "내용에 맞춰 자르기 (해제 시 최대 바운드 크기)");
            let save_btn = Button::new(format!("{:.2}s 시점 PNG로 저장", state.time))
                .min_size(egui::vec2(ui.available_width(), 30.0));
            if ui.add(save_btn).clicked() {
                action.save_frame = true;
            }
            if let Some(msg) = &state.export_status {
                ui.label(msg);
            }

            ui.add_space(8.0);
            ui.separator();
        }

        // 4. PNG 시퀀스 녹화 섹션
        ui.heading("PNG 시퀀스 녹화");
        ui.add_space(4.0);

        if !state.is_recording {
            let btn = Button::new("녹화 시작").min_size(egui::vec2(ui.available_width(), 30.0));
            if ui.add(btn).clicked() {
                action.start_recording = true;
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("FPS");
                ui.add(
                    DragValue::new(&mut state.recording_fps)
                        .speed(1.0)
                        .range(0.01..=60.0)
                        .max_decimals(2),
                );
            });

            ui.horizontal(|ui| {
                ui.label("해상도 배율");
                ui.add(
                    DragValue::new(&mut state.render_scale)
                        .speed(0.05)
                        .range(0.01..=3.0)
                        .max_decimals(2),
                );
            });

            ui.add_space(2.0);
            ui.label("저장 폴더");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.recording_path);
                if ui.button("찾기...").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        state.recording_path = folder.to_string_lossy().into_owned();
                    }
                }
            });
        } else {
            let btn = Button::new("녹화 중지").min_size(egui::vec2(ui.available_width(), 30.0));
            if ui.add(btn).clicked() {
                action.stop_recording = true;
            }

            let progress = if duration > 0.0 {
                (state.time / duration).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ui.add_space(4.0);
            ui.add(ProgressBar::new(progress).show_percentage());
            ui.label(format!("저장된 프레임: {}", state.current_recording_frame));
        }

        ui.add_space(10.0);
    });

    action
}
