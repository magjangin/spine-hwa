use std::path::{Path, PathBuf};

pub struct Recorder;

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

    /// 알파가 0이 아닌 픽셀의 외접 사각형으로 잘라낸다. 완전히 투명하면 None.
    pub fn crop_to_content(width: u32, height: u32, rgba: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
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
        if !found {
            return None;
        }
        let cw = max_x - min_x + 1;
        let ch = max_y - min_y + 1;
        let mut out = Vec::with_capacity(cw * ch * 4);
        for y in min_y..=max_y {
            let start = (y * w + min_x) * 4;
            out.extend_from_slice(&rgba[start..start + cw * 4]);
        }
        Some((cw as u32, ch as u32, out))
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
        let safe_name: String = anim_name
            .chars()
            .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
            .collect();
        let path = dir.join(format!("{}_{:.3}s.png", safe_name, time));
        image::save_buffer(&path, &data, w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("저장 실패: {}", e))?;
        Ok(path)
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
