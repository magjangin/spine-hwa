//! 스켈레톤을 FBO에 그려 PNG/시트로 내보내는 공용 로직 (GUI 앱과 헤드리스 CLI가 함께 쓴다).

use eframe::glow;

use crate::recorder::{Recorder, SheetBuilder};
use crate::renderer::GlRenderer;
use crate::spine_wrapper::SpineSkeleton;

/// 스켈레톤을 `time` 시점 포즈로 FBO에 그리고 RGBA(위가 첫 행) 픽셀을 읽는다.
pub fn render_pose(
    gl: &glow::Context,
    renderer: &GlRenderer,
    sk: &SpineSkeleton,
    time: f32,
    pma: bool,
) -> Vec<u8> {
    sk.set_time(time);
    sk.update(0.0);
    let mesh = sk.generate_mesh(pma);
    let mut pixels = unsafe {
        renderer.render_to_fbo(gl, mesh.vertices, mesh.indices, mesh.batches, pma);
        renderer.read_fbo_pixels(gl)
    };
    // PMA로 그린 FBO는 알파가 미리 곱해져 있으므로 저장용(straight alpha)으로 되돌린다.
    if pma {
        Recorder::unpremultiply(&mut pixels);
    }
    pixels
}

pub struct SheetOptions {
    pub fps: f32,
    pub cols: u32,
    pub scale: f32,
    /// true면 전 프레임의 실제 내용 합집합 영역으로 셀을 줄인다.
    pub crop: bool,
    /// 합집합 영역을 구할 때 본체에서 먼 납작한 그림자 덩어리를 뺀다.
    pub trim_shadow: bool,
    pub pma: bool,
}

pub struct SheetResult {
    pub cell_w: u32,
    pub cell_h: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub scale: f32,
    pub cols: u32,
    pub rows: u32,
    pub count: u32,
    pub fps: f32,
    pub duration: f32,
}

impl SheetResult {
    pub fn json(&self, image_name: &str) -> String {
        format!(
            "{{\n  \"image\": \"{}\",\n  \"frameWidth\": {},\n  \"frameHeight\": {},\n  \"offsetX\": {},\n  \"offsetY\": {},\n  \"scale\": {},\n  \"columns\": {},\n  \"rows\": {},\n  \"frameCount\": {},\n  \"fps\": {},\n  \"duration\": {}\n}}\n",
            image_name,
            self.cell_w,
            self.cell_h,
            self.offset_x,
            self.offset_y,
            self.scale,
            self.cols,
            self.rows,
            self.count,
            self.fps,
            self.duration
        )
    }
}

/// 현재 애니메이션 전 구간을 `fps` 간격으로 렌더해 격자 시트를 만든다.
/// 끝나면 스켈레톤 시간을 `restore_time`으로 되돌린다.
pub fn render_sheet(
    gl: &glow::Context,
    renderer: &GlRenderer,
    sk: &SpineSkeleton,
    opts: &SheetOptions,
    restore_time: f32,
) -> Result<(SheetBuilder, SheetResult), String> {
    let (fbo_w, fbo_h) = (renderer.fbo_width as u32, renderer.fbo_height as u32);
    if fbo_w == 0 || fbo_h == 0 {
        return Err("렌더 영역이 비어 있습니다.".to_string());
    }

    let fps = opts.fps.clamp(1.0, 60.0);
    let duration = sk.get_duration().max(0.001);
    let count = ((duration * fps).ceil() as u32).max(1);
    let frame = |i: u32| render_pose(gl, renderer, sk, i as f32 / fps, opts.pma);

    // 1패스: 전 프레임의 실제 내용 합집합 영역을 구해 셀 크기로 쓴다 (최대 바운드보다 훨씬 작을 수 있음).
    let rect = if opts.crop {
        let mut union: Option<(u32, u32, u32, u32)> = None;
        for i in 0..count {
            if let Some(info) = Recorder::analyze_content(fbo_w, fbo_h, &frame(i), opts.trim_shadow) {
                let b = info.rect;
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

    let cols = opts.cols.clamp(1, count);
    let rows = (count + cols - 1) / cols;

    // 사용자 배율, 그리고 한계(한 변/총 픽셀)를 넘으면 자동으로 더 줄인다.
    let full_w = (src_w * cols) as f32;
    let full_h = (src_h * rows) as f32;
    let fit_side = (SheetBuilder::MAX_SIDE as f32 / full_w).min(SheetBuilder::MAX_SIDE as f32 / full_h);
    let fit_area = (SheetBuilder::MAX_PIXELS as f32 / (full_w * full_h)).sqrt();
    let scale = opts.scale.clamp(0.01, 1.0).min(fit_side).min(fit_area);
    let cell_w = ((src_w as f32 * scale).round() as u32).max(1);
    let cell_h = ((src_h as f32 * scale).round() as u32).max(1);
    let mut sheet = SheetBuilder::new(cell_w, cell_h, cols, rows)?;

    // 2패스: 각 프레임을 합집합 영역으로 잘라(필요하면 축소해) 시트에 배치
    for i in 0..count {
        let cropped = Recorder::crop_rect(fbo_w, &frame(i), rect);
        if cell_w == src_w && cell_h == src_h {
            sheet.blit(i, &cropped);
        } else {
            let img = image::RgbaImage::from_raw(src_w, src_h, cropped).ok_or("프레임 버퍼 크기 오류")?;
            let small = image::imageops::resize(&img, cell_w, cell_h, image::imageops::FilterType::Lanczos3);
            sheet.blit(i, small.as_raw());
        }
    }

    sk.set_time(restore_time);
    sk.update(0.0);

    Ok((
        sheet,
        SheetResult {
            cell_w,
            cell_h,
            offset_x: rect.0,
            offset_y: rect.1,
            scale,
            cols,
            rows,
            count,
            fps,
            duration,
        },
    ))
}
