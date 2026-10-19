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
