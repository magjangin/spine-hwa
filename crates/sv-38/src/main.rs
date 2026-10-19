#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate spine_sys_38;

fn main() {
    spine_viewer_core::run();
}
