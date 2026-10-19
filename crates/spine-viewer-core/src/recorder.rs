use std::path::{Path, PathBuf};

pub struct Recorder;

/// 같은 크기의 셀을 격자로 배치하는 애니메이션 시트 버퍼.
pub struct SheetBuilder {
    pub cell_w: u32,
    pub cell_h: u32,
    pub cols: u32,
    pub rows: u32,
    pixels: Vec<u8>,
}

impl SheetBuilder {
    pub const MAX_SIDE: u32 = 16384;
    /// 시트 버퍼(RGBA) 메모리를 약 400MB 이하로 묶는 총 픽셀 한계.
    pub const MAX_PIXELS: u64 = 100_000_000;

    pub fn new(cell_w: u32, cell_h: u32, cols: u32, rows: u32) -> Result<Self, String> {
        let w = cell_w as u64 * cols as u64;
        let h = cell_h as u64 * rows as u64;
        if w > Self::MAX_SIDE as u64 || h > Self::MAX_SIDE as u64 || w * h > Self::MAX_PIXELS + Self::MAX_PIXELS / 50 {
            return Err(format!(
                "시트 크기 {}x{} 가 한계({})를 넘습니다. 열 수/FPS를 조정하세요.",
                w, h, Self::MAX_SIDE
            ));
        }
        Ok(Self {
            cell_w,
            cell_h,
            cols,
            rows,
            pixels: vec![0u8; (w * h * 4) as usize],
        })
    }

    pub fn width(&self) -> u32 {
        self.cell_w * self.cols
    }

    pub fn height(&self) -> u32 {
        self.cell_h * self.rows
    }

    /// `index` 번째 셀에 셀 크기의 RGBA 프레임을 복사한다.
    pub fn blit(&mut self, index: u32, frame: &[u8]) {
        let (cw, ch) = (self.cell_w as usize, self.cell_h as usize);
        if frame.len() < cw * ch * 4 {
            return;
        }
        let sheet_w = self.width() as usize;
        let x0 = (index % self.cols) as usize * cw;
        let y0 = (index / self.cols) as usize * ch;
        for y in 0..ch {
            let dst = ((y0 + y) * sheet_w + x0) * 4;
            let src = y * cw * 4;
            self.pixels[dst..dst + cw * 4].copy_from_slice(&frame[src..src + cw * 4]);
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        image::save_buffer(
            path,
            &self.pixels,
            self.width(),
            self.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("저장 실패: {}", e))
    }
}

impl Recorder {
    pub fn validate_path(path_str: &str) -> Result<PathBuf, String> {
        let actual_path = if path_str.trim().is_empty() {
            Path::new(".")
        } else {
            Path::new(path_str.trim())
        };

        if !actual_path.is_dir() {
            let msg = format!(
                "The supplied recording path '{}' is not a directory! Check the path for errors and make sure the directory exists.",
                actual_path.display()
            );
            eprintln!("{}", msg);
            return Err(msg);
        }

        Ok(actual_path.to_path_buf())
    }

    /// 알파가 0이 아닌 픽셀의 외접 사각형 `(min_x, min_y, max_x, max_y)` (끝 포함). 완전히 투명하면 None.
    pub fn content_bounds(width: u32, height: u32, rgba: &[u8]) -> Option<(u32, u32, u32, u32)> {
        let (w, h) = (width as usize, height as usize);
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0usize, 0usize);
        let mut found = false;
        for y in 0..h {
            for x in 0..w {
                if rgba[(y * w + x) * 4 + 3] != 0 {
                    found = true;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        found.then_some((min_x as u32, min_y as u32, max_x as u32, max_y as u32))
    }

    /// `width` 폭의 RGBA 버퍼에서 `rect`(끝 포함) 영역만 복사한다.
    pub fn crop_rect(width: u32, rgba: &[u8], rect: (u32, u32, u32, u32)) -> Vec<u8> {
        let (x0, y0, x1, y1) = (rect.0 as usize, rect.1 as usize, rect.2 as usize, rect.3 as usize);
        let w = width as usize;
        let cw = x1 - x0 + 1;
        let mut out = Vec::with_capacity(cw * (y1 - y0 + 1) * 4);
        for y in y0..=y1 {
            let start = (y * w + x0) * 4;
            out.extend_from_slice(&rgba[start..start + cw * 4]);
        }
        out
    }

    /// 알파가 0이 아닌 픽셀의 외접 사각형으로 잘라낸다. 완전히 투명하면 None.
    pub fn crop_to_content(width: u32, height: u32, rgba: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let rect = Self::content_bounds(width, height, rgba)?;
        let data = Self::crop_rect(width, rgba, rect);
        Some((rect.2 - rect.0 + 1, rect.3 - rect.1 + 1, data))
    }

    /// 현재 프레임 1장을 `<애니메이션>_<시간>s.png` 로 저장하고 저장 경로를 돌려준다.
    pub fn save_snapshot(
        dir: &Path,
        anim_name: &str,
        time: f32,
        crop: bool,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
    ) -> Result<PathBuf, String> {
        if rgba_pixels.is_empty() {
            return Err("렌더 결과가 비어 있습니다.".to_string());
        }
        let (w, h, data) = if crop {
            Self::crop_to_content(width, height, rgba_pixels)
                .ok_or_else(|| "이 시점에는 보이는 픽셀이 없습니다.".to_string())?
        } else {
            (width, height, rgba_pixels.to_vec())
        };
        let safe_name = Self::sanitize(anim_name);
        let path = dir.join(format!("{}_{:.3}s.png", safe_name, time));
        image::save_buffer(&path, &data, w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("저장 실패: {}", e))?;
        Ok(path)
    }

    /// 파일명에 쓸 수 없는 문자를 `_` 로 바꾼다.
    pub fn sanitize(name: &str) -> String {
        name.chars()
            .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
            .collect()
    }

    pub fn save_frame(
        dir: &Path,
        anim_name: &str,
        frame_idx: u32,
        width: u32,
        height: u32,
        rgba_pixels: &[u8],
    ) -> Result<(), String> {
        let file_name = format!("{}_{}.png", anim_name, frame_idx);
        let file_path = dir.join(file_name);

        image::save_buffer(
            &file_path,
            rgba_pixels,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("Failed to save frame: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_places_cells_in_grid() {
        let mut sheet = SheetBuilder::new(2, 2, 2, 2).unwrap();
        for i in 0..3u32 {
            sheet.blit(i, &vec![(i + 1) as u8; 2 * 2 * 4]);
        }
        let w = sheet.width() as usize;
        let at = |x: usize, y: usize| sheet.pixels[(y * w + x) * 4];
        assert_eq!((at(0, 0), at(2, 0), at(0, 2), at(2, 2)), (1, 2, 3, 0));
        assert!(SheetBuilder::new(9000, 10, 2, 1).is_err());
    }

    #[test]
    fn crop_finds_content_bounds() {
        // 4x3 투명 이미지, (1,1)~(2,2)에 불투명 픽셀
        let (w, h) = (4u32, 3u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for (x, y) in [(1usize, 1usize), (2, 2)] {
            px[(y * w as usize + x) * 4 + 3] = 255;
        }
        let (cw, ch, data) = Recorder::crop_to_content(w, h, &px).unwrap();
        assert_eq!((cw, ch), (2, 2));
        assert_eq!(data[3], 255); // 좌상단 = (1,1)
        assert_eq!(data[(3 * 4) + 3], 255); // 우하단 = (2,2)
        assert!(Recorder::crop_to_content(w, h, &vec![0u8; px.len()]).is_none());
    }
}
