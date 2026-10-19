use std::ffi::c_void;
use std::os::raw::{c_char, c_int};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SpineVertex {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    pub color: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SpineDrawBatch {
    pub texture_index: i32,
    pub blend_mode: i32,
    pub start_index: u32,
    pub index_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SpineRenderData {
    pub vertices: *const SpineVertex,
    pub vertex_count: u32,
    pub indices: *const u16,
    pub index_count: u32,
    pub batches: *const SpineDrawBatch,
    pub batch_count: u32,
}

pub type SpineHandle = *mut c_void;

extern "C" {
    pub fn spine_create(
        atlas_path: *const c_char,
        data_path: *const c_char,
        is_json: bool,
        error_buf: *mut c_char,
        error_buf_len: c_int,
    ) -> SpineHandle;

    pub fn spine_destroy(handle: SpineHandle);

    pub fn spine_get_page_count(handle: SpineHandle) -> i32;
    pub fn spine_get_page_path(handle: SpineHandle, page_index: i32) -> *const c_char;
    pub fn spine_set_page_texture_size(handle: SpineHandle, page_index: i32, width: i32, height: i32);

    pub fn spine_get_animation_count(handle: SpineHandle) -> i32;
    pub fn spine_get_animation_name(handle: SpineHandle, index: i32) -> *const c_char;
    pub fn spine_get_animation_duration_by_index(handle: SpineHandle, index: i32) -> f32;

    pub fn spine_get_skin_count(handle: SpineHandle) -> i32;
    pub fn spine_get_skin_name(handle: SpineHandle, index: i32) -> *const c_char;

    pub fn spine_set_animation(handle: SpineHandle, name: *const c_char, loop_: bool) -> bool;
    pub fn spine_set_skin(handle: SpineHandle, name: *const c_char) -> bool;
    pub fn spine_set_time(handle: SpineHandle, time: f32);
    pub fn spine_get_time(handle: SpineHandle) -> f32;
    pub fn spine_get_duration(handle: SpineHandle) -> f32;
    pub fn spine_set_loop(handle: SpineHandle, loop_: bool);
    pub fn spine_get_loop(handle: SpineHandle) -> bool;
    pub fn spine_set_timescale(handle: SpineHandle, timescale: f32);
    pub fn spine_get_timescale(handle: SpineHandle) -> f32;
    pub fn spine_set_position(handle: SpineHandle, x: f32, y: f32);
    pub fn spine_get_position(handle: SpineHandle, out_x: *mut f32, out_y: *mut f32);
    pub fn spine_set_scale(handle: SpineHandle, scale_x: f32, scale_y: f32);

    pub fn spine_update(handle: SpineHandle, dt: f32);
    pub fn spine_apply_setup_pose(handle: SpineHandle);

    pub fn spine_compute_max_bounds(
        handle: SpineHandle,
        anim_name: *const c_char,
        precision: f32,
        out_x: *mut f32,
        out_y: *mut f32,
        out_w: *mut f32,
        out_h: *mut f32,
    ) -> bool;

    pub fn spine_generate_mesh(handle: SpineHandle, pma: bool) -> *const SpineRenderData;
}
