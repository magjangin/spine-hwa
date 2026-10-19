use eframe::glow::{self, HasContext};
use std::path::Path;
use spine_sys_38::{SpineDrawBatch, SpineVertex};

pub struct GlRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    u_proj: glow::UniformLocation,
    u_texture: glow::UniformLocation,
    textures: Vec<glow::Texture>,

    // Offscreen render texture
    fbo: glow::Framebuffer,
    fbo_texture: glow::Texture,
    pub fbo_width: i32,
    pub fbo_height: i32,
}

impl GlRenderer {
    pub unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let vs_source = r#"#version 330 core
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec4 a_color;

uniform mat4 u_proj;

out vec2 v_uv;
out vec4 v_color;

void main() {
    v_uv = a_uv;
    v_color = a_color;
    gl_Position = u_proj * vec4(a_pos, 0.0, 1.0);
}
"#;

        let fs_source = r#"#version 330 core
in vec2 v_uv;
in vec4 v_color;

uniform sampler2D u_texture;

out vec4 FragColor;

void main() {
    vec4 texColor = texture(u_texture, v_uv);
    FragColor = texColor * v_color;
}
"#;

        let vs = gl.create_shader(glow::VERTEX_SHADER).map_err(|e| e.to_string())?;
        gl.shader_source(vs, vs_source);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            return Err(format!("Vertex shader compile error: {}", log));
        }

        let fs = gl.create_shader(glow::FRAGMENT_SHADER).map_err(|e| e.to_string())?;
        gl.shader_source(fs, fs_source);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            return Err(format!("Fragment shader compile error: {}", log));
        }

        let program = gl.create_program().map_err(|e| e.to_string())?;
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            return Err(format!("Program link error: {}", log));
        }

        gl.delete_shader(vs);
        gl.delete_shader(fs);

        let u_proj = gl.get_uniform_location(program, "u_proj").ok_or("u_proj not found")?;
        let u_texture = gl.get_uniform_location(program, "u_texture").ok_or("u_texture not found")?;

        let vao = gl.create_vertex_array().map_err(|e| e.to_string())?;
        let vbo = gl.create_buffer().map_err(|e| e.to_string())?;
        let ebo = gl.create_buffer().map_err(|e| e.to_string())?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));

        let stride = std::mem::size_of::<SpineVertex>() as i32;

        // a_pos: offset 0
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);

        // a_uv: offset 8
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 8);

        // a_color: offset 16, 4 bytes RGBA normalized
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::UNSIGNED_BYTE, true, stride, 16);

        gl.bind_vertex_array(None);

        let fbo = gl.create_framebuffer().map_err(|e| e.to_string())?;
        let fbo_texture = gl.create_texture().map_err(|e| e.to_string())?;

        Ok(Self {
            program,
            vao,
            vbo,
            ebo,
            u_proj,
            u_texture,
            textures: Vec::new(),
            fbo,
            fbo_texture,
            fbo_width: 0,
            fbo_height: 0,
        })
    }

    pub unsafe fn load_textures(&mut self, gl: &glow::Context, paths: &[String]) -> Result<Vec<(i32, i32)>, String> {
        let mut sizes = Vec::new();

        for path_str in paths {
            let path = Path::new(path_str);
            let img = image::open(path)
                .map_err(|e| format!("Failed to open texture image {}: {}", path.display(), e))?
                .to_rgba8();

            let (w, h) = img.dimensions();
            sizes.push((w as i32, h as i32));

            let tex = gl.create_texture().map_err(|e| e.to_string())?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));

            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&img.into_raw())),
            );

            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

            gl.bind_texture(glow::TEXTURE_2D, None);
            self.textures.push(tex);
        }

        Ok(sizes)
    }

    pub unsafe fn resize_render_texture(&mut self, gl: &glow::Context, width: i32, height: i32) -> Result<(), String> {
        let w = width.max(1);
        let h = height.max(1);

        if self.fbo_width == w && self.fbo_height == h {
            return Ok(());
        }

        self.fbo_width = w;
        self.fbo_height = h;

        gl.bind_texture(glow::TEXTURE_2D, Some(self.fbo_texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );

        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(self.fbo_texture),
            0,
        );

        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);

        if status != glow::FRAMEBUFFER_COMPLETE {
            return Err("Failed to create complete framebuffer".to_string());
        }

        Ok(())
    }

    /// egui에 `frame.register_native_glow_texture`로 등록할 FBO 색상 텍스처.
    /// (GL 원시 id를 `TextureId::User`에 직접 넣으면 egui가 모르는 id라 그려지지 않는다.)
    pub fn fbo_texture(&self) -> glow::Texture {
        self.fbo_texture
    }

    pub unsafe fn render_to_fbo(
        &self,
        gl: &glow::Context,
        vertices: &[SpineVertex],
        indices: &[u16],
        batches: &[SpineDrawBatch],
        pma: bool,
    ) {
        if self.fbo_width <= 0 || self.fbo_height <= 0 {
            return;
        }

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
        gl.viewport(0, 0, self.fbo_width, self.fbo_height);

        // Clear with transparent color (§6.4)
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(glow::COLOR_BUFFER_BIT);

        if vertices.is_empty() || indices.is_empty() || batches.is_empty() {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            return;
        }

        gl.use_program(Some(self.program));

        // Orthographic projection matrix Y-down: (0, 0) top-left to (w, h) bottom-right
        let w = self.fbo_width as f32;
        let h = self.fbo_height as f32;
        #[rustfmt::skip]
        let ortho: [f32; 16] = [
            2.0 / w,  0.0,      0.0, 0.0,
            0.0,     -2.0 / h,  0.0, 0.0,
            0.0,      0.0,     -1.0, 0.0,
           -1.0,      1.0,      0.0, 1.0,
        ];
        gl.uniform_matrix_4_f32_slice(Some(&self.u_proj), false, &ortho);

        gl.uniform_1_i32(Some(&self.u_texture), 0);
        gl.active_texture(glow::TEXTURE0);

        gl.bind_vertex_array(Some(self.vao));

        // Upload vertex buffer
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
        let vertex_bytes = std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            vertices.len() * std::mem::size_of::<SpineVertex>(),
        );
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::DYNAMIC_DRAW);

        // Upload index buffer
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
        let index_bytes = std::slice::from_raw_parts(
            indices.as_ptr() as *const u8,
            indices.len() * std::mem::size_of::<u16>(),
        );
        gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_bytes, glow::DYNAMIC_DRAW);

        gl.enable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);

        for batch in batches {
            let tex_idx = batch.texture_index as usize;
            if tex_idx >= self.textures.len() {
                continue;
            }

            gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[tex_idx]));

            // Set blend mode (§9.3)
            // 0: Normal, 1: Additive, 2: Multiply, 3: Screen
            match batch.blend_mode {
                0 => {
                    // Normal
                    if pma {
                        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                    } else {
                        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                    }
                }
                1 => {
                    // Additive
                    if pma {
                        gl.blend_func(glow::ONE, glow::ONE);
                    } else {
                        gl.blend_func(glow::SRC_ALPHA, glow::ONE);
                    }
                }
                2 => {
                    // Multiply
                    gl.blend_func(glow::DST_COLOR, glow::ONE_MINUS_SRC_ALPHA);
                }
                3 => {
                    // Screen
                    gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_COLOR);
                }
                _ => {
                    if pma {
                        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                    } else {
                        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                    }
                }
            }

            let byte_offset = (batch.start_index as usize * std::mem::size_of::<u16>()) as i32;
            gl.draw_elements(
                glow::TRIANGLES,
                batch.index_count as i32,
                glow::UNSIGNED_SHORT,
                byte_offset,
            );
        }

        gl.bind_vertex_array(None);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }

    pub unsafe fn read_fbo_pixels(&self, gl: &glow::Context) -> Vec<u8> {
        let w = self.fbo_width;
        let h = self.fbo_height;
        if w <= 0 || h <= 0 {
            return Vec::new();
        }

        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));

        let mut raw_pixels = vec![0u8; (w * h * 4) as usize];
        gl.read_pixels(
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut raw_pixels)),
        );

        gl.bind_framebuffer(glow::FRAMEBUFFER, None);

        // OpenGL reads from bottom-to-top, so flip vertically for PNG saving
        let row_bytes = (w * 4) as usize;
        let mut flipped = vec![0u8; raw_pixels.len()];
        for y in 0..h as usize {
            let src_row = (h as usize - 1 - y) * row_bytes;
            let dst_row = y * row_bytes;
            flipped[dst_row..dst_row + row_bytes].copy_from_slice(&raw_pixels[src_row..src_row + row_bytes]);
        }

        flipped
    }

    pub unsafe fn clear_textures(&mut self, gl: &glow::Context) {
        for tex in self.textures.drain(..) {
            gl.delete_texture(tex);
        }
    }

    pub unsafe fn destroy(&self, gl: &glow::Context) {
        gl.delete_framebuffer(self.fbo);
        gl.delete_texture(self.fbo_texture);
        for tex in &self.textures {
            gl.delete_texture(*tex);
        }
        gl.delete_buffer(self.vbo);
        gl.delete_buffer(self.ebo);
        gl.delete_vertex_array(self.vao);
        gl.delete_program(self.program);
    }
}
