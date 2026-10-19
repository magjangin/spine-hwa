#ifndef SPINE_BRIDGE_H
#define SPINE_BRIDGE_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    float x;
    float y;
    float u;
    float v;
    uint32_t color; // RGBA 8:8:8:8 (Little endian: R, G, B, A in bytes)
} SpineVertex;

typedef struct {
    int32_t texture_index; // Atlas page index (0, 1, ...)
    int32_t blend_mode;    // 0: Normal, 1: Additive, 2: Multiply, 3: Screen
    uint32_t start_index;  // Starting index in indices buffer
    uint32_t index_count;  // Number of indices in this batch
} SpineDrawBatch;

typedef struct {
    const SpineVertex* vertices;
    uint32_t vertex_count;
    const uint16_t* indices;
    uint32_t index_count;
    const SpineDrawBatch* batches;
    uint32_t batch_count;
} SpineRenderData;

typedef void* SpineHandle;

// Lifecycle
SpineHandle spine_create(const char* atlas_path, const char* data_path, bool is_json, char* error_buf, int error_buf_len);
void spine_destroy(SpineHandle handle);

// Atlas pages
int32_t spine_get_page_count(SpineHandle handle);
const char* spine_get_page_path(SpineHandle handle, int32_t page_index);
void spine_set_page_texture_size(SpineHandle handle, int32_t page_index, int32_t width, int32_t height);

// Animations & Skins
int32_t spine_get_animation_count(SpineHandle handle);
const char* spine_get_animation_name(SpineHandle handle, int32_t index);
float spine_get_animation_duration_by_index(SpineHandle handle, int32_t index);

int32_t spine_get_skin_count(SpineHandle handle);
const char* spine_get_skin_name(SpineHandle handle, int32_t index);

// State Controls
bool spine_set_animation(SpineHandle handle, const char* name, bool loop);
bool spine_set_skin(SpineHandle handle, const char* name);
void spine_set_time(SpineHandle handle, float time);
float spine_get_time(SpineHandle handle);
float spine_get_duration(SpineHandle handle);
void spine_set_loop(SpineHandle handle, bool loop);
bool spine_get_loop(SpineHandle handle);
void spine_set_timescale(SpineHandle handle, float timescale);
float spine_get_timescale(SpineHandle handle);
void spine_set_position(SpineHandle handle, float x, float y);
void spine_get_position(SpineHandle handle, float* out_x, float* out_y);
void spine_set_scale(SpineHandle handle, float scale_x, float scale_y);

// Update
void spine_update(SpineHandle handle, float dt);
void spine_apply_setup_pose(SpineHandle handle);

// Bounds & Mesh
bool spine_compute_max_bounds(SpineHandle handle, const char* anim_name, float precision,
                             float* out_x, float* out_y, float* out_w, float* out_h);

const SpineRenderData* spine_generate_mesh(SpineHandle handle, bool pma);

#ifdef __cplusplus
}
#endif

#endif // SPINE_BRIDGE_H
