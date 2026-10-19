use spine_sys_38::*;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct SpineSkeleton {
    handle: SpineHandle,
}

// SpineHandle contains raw pointers, safe to transfer across threads if synchronized
unsafe impl Send for SpineSkeleton {}
unsafe impl Sync for SpineSkeleton {}

impl SpineSkeleton {
    pub fn load(atlas_path: &Path, data_path: &Path, is_json: bool) -> Result<Self, String> {
        if !atlas_path.is_file() || !data_path.is_file() {
            return Err("invalid atlas/data file path".to_string());
        }

        let c_atlas = CString::new(atlas_path.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        let c_data = CString::new(data_path.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;

        let mut error_buf = vec![0 as c_char; 512];

        let handle = unsafe {
            spine_create(
                c_atlas.as_ptr(),
                c_data.as_ptr(),
                is_json,
                error_buf.as_mut_ptr(),
                error_buf.len() as i32,
            )
        };

        if handle.is_null() {
            let err_str = unsafe {
                CStr::from_ptr(error_buf.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(if err_str.is_empty() {
                "invalid data file".to_string()
            } else {
                err_str
            });
        }

        Ok(Self { handle })
    }

    pub fn get_page_count(&self) -> i32 {
        unsafe { spine_get_page_count(self.handle) }
    }

    pub fn get_page_path(&self, index: i32) -> String {
        unsafe {
            let ptr = spine_get_page_path(self.handle, index);
            if ptr.is_null() {
                String::new()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    pub fn set_page_texture_size(&self, index: i32, width: i32, height: i32) {
        unsafe { spine_set_page_texture_size(self.handle, index, width, height) }
    }

    pub fn get_animations(&self) -> Vec<String> {
        let count = unsafe { spine_get_animation_count(self.handle) };
        let mut anims = Vec::with_capacity(count as usize);
        for i in 0..count {
            unsafe {
                let ptr = spine_get_animation_name(self.handle, i);
                if !ptr.is_null() {
                    anims.push(CStr::from_ptr(ptr).to_string_lossy().into_owned());
                }
            }
        }
        anims
    }

    pub fn get_skins(&self) -> Vec<String> {
        let count = unsafe { spine_get_skin_count(self.handle) };
        let mut skins = Vec::with_capacity(count as usize);
        for i in 0..count {
            unsafe {
                let ptr = spine_get_skin_name(self.handle, i);
                if !ptr.is_null() {
                    skins.push(CStr::from_ptr(ptr).to_string_lossy().into_owned());
                }
            }
        }
        skins
    }

    pub fn set_animation(&self, name: &str, loop_: bool) -> bool {
        if let Ok(c_name) = CString::new(name) {
            unsafe { spine_set_animation(self.handle, c_name.as_ptr(), loop_) }
        } else {
            false
        }
    }

    pub fn set_skin(&self, name: &str) -> bool {
        if let Ok(c_name) = CString::new(name) {
            unsafe { spine_set_skin(self.handle, c_name.as_ptr()) }
        } else {
            false
        }
    }

    pub fn set_time(&self, time: f32) {
        unsafe { spine_set_time(self.handle, time) }
    }

    pub fn get_time(&self) -> f32 {
        unsafe { spine_get_time(self.handle) }
    }

    pub fn get_duration(&self) -> f32 {
        unsafe { spine_get_duration(self.handle) }
    }

    pub fn set_loop(&self, loop_: bool) {
        unsafe { spine_set_loop(self.handle, loop_) }
    }

    pub fn get_loop(&self) -> bool {
        unsafe { spine_get_loop(self.handle) }
    }

    pub fn set_timescale(&self, timescale: f32) {
        unsafe { spine_set_timescale(self.handle, timescale) }
    }

    pub fn get_timescale(&self) -> f32 {
        unsafe { spine_get_timescale(self.handle) }
    }

    pub fn set_position(&self, x: f32, y: f32) {
        unsafe { spine_set_position(self.handle, x, y) }
    }

    pub fn get_position(&self) -> (f32, f32) {
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        unsafe { spine_get_position(self.handle, &mut x, &mut y) };
        (x, y)
    }

    pub fn set_scale(&self, scale_x: f32, scale_y: f32) {
        unsafe { spine_set_scale(self.handle, scale_x, scale_y) }
    }

    pub fn update(&self, dt: f32) {
        unsafe { spine_update(self.handle, dt) }
    }

    pub fn apply_setup_pose(&self) {
        unsafe { spine_apply_setup_pose(self.handle) }
    }

    pub fn compute_max_bounds(&self, anim_name: &str, precision: f32) -> Result<Rect, String> {
        let c_name = CString::new(anim_name).map_err(|e| e.to_string())?;
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut w = 0.0f32;
        let mut h = 0.0f32;

        let ok = unsafe {
            spine_compute_max_bounds(
                self.handle,
                c_name.as_ptr(),
                precision,
                &mut x,
                &mut y,
                &mut w,
                &mut h,
            )
        };

        if ok {
            Ok(Rect {
                x,
                y,
                width: w,
                height: h,
            })
        } else {
            Err("failed to compute max bounds".to_string())
        }
    }

    pub fn generate_mesh(&self, pma: bool) -> RenderDataView<'_> {
        let ptr = unsafe { spine_generate_mesh(self.handle, pma) };
        if ptr.is_null() {
            RenderDataView {
                vertices: &[],
                indices: &[],
                batches: &[],
            }
        } else {
            unsafe {
                let rd = &*ptr;
                let vertices = if rd.vertex_count > 0 && !rd.vertices.is_null() {
                    std::slice::from_raw_parts(rd.vertices, rd.vertex_count as usize)
                } else {
                    &[]
                };
                let indices = if rd.index_count > 0 && !rd.indices.is_null() {
                    std::slice::from_raw_parts(rd.indices, rd.index_count as usize)
                } else {
                    &[]
                };
                let batches = if rd.batch_count > 0 && !rd.batches.is_null() {
                    std::slice::from_raw_parts(rd.batches, rd.batch_count as usize)
                } else {
                    &[]
                };
                RenderDataView {
                    vertices,
                    indices,
                    batches,
                }
            }
        }
    }
}

pub struct RenderDataView<'a> {
    pub vertices: &'a [SpineVertex],
    pub indices: &'a [u16],
    pub batches: &'a [SpineDrawBatch],
}

impl Drop for SpineSkeleton {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { spine_destroy(self.handle) }
        }
    }
}
