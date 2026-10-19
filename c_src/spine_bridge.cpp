#include "spine_bridge.h"

#include <spine/spine.h>
#include <vector>
#include <string>
#include <cstring>
#include <cstdio>
#include <cmath>
#include <algorithm>

using namespace spine;

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

static FILE* utf8_fopen(const char* utf8_path, const wchar_t* mode) {
    if (!utf8_path) return nullptr;
    int wlen = MultiByteToWideChar(CP_UTF8, 0, utf8_path, -1, nullptr, 0);
    if (wlen <= 0) return fopen(utf8_path, "rb");
    std::vector<wchar_t> wpath(wlen);
    MultiByteToWideChar(CP_UTF8, 0, utf8_path, -1, wpath.data(), wlen);
    return _wfopen(wpath.data(), mode);
}
#else
static FILE* utf8_fopen(const char* utf8_path, const char* mode) {
    return fopen(utf8_path, mode);
}
#endif

// Spine 3.8 UTF-8 파일 지원 확장
class Utf8SpineExtension : public DefaultSpineExtension {
public:
    virtual char* _readFile(const String& path, int* length) override {
#ifdef _WIN32
        FILE* file = utf8_fopen(path.buffer(), L"rb");
#else
        FILE* file = fopen(path.buffer(), "rb");
#endif
        if (!file) return nullptr;
        fseek(file, 0, SEEK_END);
        *length = (int)ftell(file);
        fseek(file, 0, SEEK_SET);

        char* data = SpineExtension::alloc<char>(*length, __FILE__, __LINE__);
        fread(data, 1, *length, file);
        fclose(file);
        return data;
    }
};

namespace spine {
    SpineExtension* getDefaultExtension() {
        return new Utf8SpineExtension();
    }
}

struct BridgeAtlasPageInfo {
    std::string path;
    int width, height;
    AtlasPage* page;
};

class BridgeTextureLoader : public TextureLoader {
public:
    std::vector<BridgeAtlasPageInfo> pages;

    virtual void load(AtlasPage& page, const String& path) override {
        int page_idx = (int)pages.size();
        pages.push_back({path.buffer(), page.width, page.height, &page});
        page.setRendererObject((void*)(uintptr_t)(page_idx + 1));
    }
    virtual void unload(void*) override {}
};

struct SpineInstance {
    BridgeTextureLoader textureLoader;
    Atlas* atlas = nullptr;
    SkeletonData* skeletonData = nullptr;
    Skeleton* skeleton = nullptr;
    AnimationStateData* animStateData = nullptr;
    AnimationState* animState = nullptr;

    std::vector<SpineVertex> vertices;
    std::vector<uint16_t> indices;
    std::vector<SpineDrawBatch> batches;
    SpineRenderData renderData{};

    Vector<float> worldVerticesTemp;
    SkeletonClipping clipper;

    ~SpineInstance() {
        delete animState;
        delete animStateData;
        delete skeleton;
        delete skeletonData;
        delete atlas;
    }
};

extern "C" {

SpineHandle spine_create(const char* atlas_path, const char* data_path, bool is_json, char* error_buf, int error_buf_len) {
    if (!atlas_path || !data_path) {
        if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "경로가 지정되지 않았습니다.");
        return nullptr;
    }

    Bone::setYDown(true);
    SpineInstance* inst = new SpineInstance();

    try {
        inst->atlas = new Atlas(String(atlas_path), &inst->textureLoader);
    } catch (...) {
        if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "아틀라스 파일 로드 실패");
        delete inst;
        return nullptr;
    }

    if (!inst->atlas) {
        if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "아틀라스 파일이 유효하지 않습니다.");
        delete inst;
        return nullptr;
    }

    // Spine 3.8.99 전용 버전 사전 검증
    {
        FILE* f = utf8_fopen(data_path, L"rb");
        if (f) {
            char header[512] = {0};
            size_t n = fread(header, 1, sizeof(header) - 1, f);
            fclose(f);
            char ver[16] = {0};
            bool found = false;

            if (is_json) {
                const char* k = strstr(header, "\"spine\"");
                const char* p = k ? strchr(k + 7, '"') : nullptr;
                if (p) {
                    p++;
                    size_t len = 0;
                    while (*p && *p != '"' && len < 15) ver[len++] = *p++;
                    found = len > 0;
                }
            } else if (n > 10) {
                for (size_t i = 0; i + 4 < n; ++i) {
                    if (header[i] == '3' && header[i+1] == '.' && header[i+2] == '8' && header[i+3] == '.') {
                        size_t len = 0;
                        while (i + len < n && len < 15 && header[i + len] > ' ' && header[i + len] < 127) {
                            ver[len] = header[i + len];
                            len++;
                        }
                        found = true;
                        break;
                    }
                }
            }

            if (found && strncmp(ver, "3.8", 3) != 0) {
                if (error_buf && error_buf_len > 0) {
                    snprintf(error_buf, error_buf_len, "스켈레톤 버전(%s)이 3.8.99 런타임과 일치하지 않습니다.", ver);
                }
                delete inst;
                return nullptr;
            }
        }
    }

    AtlasAttachmentLoader loader(inst->atlas);
    try {
        if (is_json) {
            SkeletonJson json(&loader);
            inst->skeletonData = json.readSkeletonDataFile(String(data_path));
            if (!inst->skeletonData) {
                if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "%s", json.getError().buffer());
                delete inst;
                return nullptr;
            }
        } else {
            SkeletonBinary binary(&loader);
            inst->skeletonData = binary.readSkeletonDataFile(String(data_path));
            if (!inst->skeletonData) {
                if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "%s", binary.getError().buffer());
                delete inst;
                return nullptr;
            }
        }
    } catch (...) {
        if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "스켈레톤 데이터 파싱 오류");
        delete inst;
        return nullptr;
    }

    if (inst->skeletonData->getAnimations().size() == 0) {
        if (error_buf && error_buf_len > 0) snprintf(error_buf, error_buf_len, "스켈레톤에 애니메이션이 없습니다.");
        delete inst;
        return nullptr;
    }

    inst->skeleton = new Skeleton(inst->skeletonData);
    inst->animStateData = new AnimationStateData(inst->skeletonData);
    inst->animState = new AnimationState(inst->animStateData);
    inst->animState->setTimeScale(1.0f);

    Animation* firstAnim = inst->skeletonData->getAnimations()[0];
    inst->animState->setAnimation(0, firstAnim, true);

    inst->skeleton->setToSetupPose();
    inst->skeleton->updateWorldTransform();

    if (inst->skeletonData->getSkins().size() > 0) {
        inst->skeleton->setSkin(inst->skeletonData->getSkins()[0]);
        inst->skeleton->setSlotsToSetupPose();
    }

    return (SpineHandle)inst;
}

void spine_destroy(SpineHandle handle) {
    delete (SpineInstance*)handle;
}

int32_t spine_get_page_count(SpineHandle handle) {
    return handle ? (int32_t)((SpineInstance*)handle)->textureLoader.pages.size() : 0;
}

const char* spine_get_page_path(SpineHandle handle, int32_t page_index) {
    if (!handle) return "";
    auto& pages = ((SpineInstance*)handle)->textureLoader.pages;
    return (page_index >= 0 && page_index < (int32_t)pages.size()) ? pages[page_index].path.c_str() : "";
}

void spine_set_page_texture_size(SpineHandle handle, int32_t page_index, int32_t width, int32_t height) {
    if (!handle) return;
    auto& pages = ((SpineInstance*)handle)->textureLoader.pages;
    if (page_index >= 0 && page_index < (int32_t)pages.size()) {
        pages[page_index].width = width;
        pages[page_index].height = height;
        if (pages[page_index].page) {
            pages[page_index].page->width = width;
            pages[page_index].page->height = height;
        }
    }
}

int32_t spine_get_animation_count(SpineHandle handle) {
    return handle ? (int32_t)((SpineInstance*)handle)->skeletonData->getAnimations().size() : 0;
}

const char* spine_get_animation_name(SpineHandle handle, int32_t index) {
    if (!handle) return "";
    auto& anims = ((SpineInstance*)handle)->skeletonData->getAnimations();
    return (index >= 0 && index < (int32_t)anims.size()) ? anims[index]->getName().buffer() : "";
}

float spine_get_animation_duration_by_index(SpineHandle handle, int32_t index) {
    if (!handle) return 0.0f;
    auto& anims = ((SpineInstance*)handle)->skeletonData->getAnimations();
    return (index >= 0 && index < (int32_t)anims.size()) ? anims[index]->getDuration() : 0.0f;
}

int32_t spine_get_skin_count(SpineHandle handle) {
    return handle ? (int32_t)((SpineInstance*)handle)->skeletonData->getSkins().size() : 0;
}

const char* spine_get_skin_name(SpineHandle handle, int32_t index) {
    if (!handle) return "";
    auto& skins = ((SpineInstance*)handle)->skeletonData->getSkins();
    return (index >= 0 && index < (int32_t)skins.size()) ? skins[index]->getName().buffer() : "";
}

bool spine_set_animation(SpineHandle handle, const char* name, bool loop) {
    if (!handle || !name) return false;
    SpineInstance* inst = (SpineInstance*)handle;
    Animation* anim = inst->skeletonData->findAnimation(String(name));
    if (!anim) return false;
    TrackEntry* current = inst->animState->getCurrent(0);
    if (current && current->getAnimation() == anim) return false;
    inst->animState->setAnimation(0, anim, loop);
    return true;
}

bool spine_set_skin(SpineHandle handle, const char* name) {
    if (!handle || !name) return false;
    SpineInstance* inst = (SpineInstance*)handle;
    Skin* skin = inst->skeletonData->findSkin(String(name));
    if (!skin || inst->skeleton->getSkin() == skin) return false;
    inst->skeleton->setSkin(skin);
    inst->skeleton->setSlotsToSetupPose();
    return true;
}

void spine_set_time(SpineHandle handle, float time) {
    if (!handle) return;
    TrackEntry* track = ((SpineInstance*)handle)->animState->getCurrent(0);
    if (track && track->getTrackTime() != time) track->setTrackTime(time);
}

float spine_get_time(SpineHandle handle) {
    if (!handle) return 0.0f;
    TrackEntry* track = ((SpineInstance*)handle)->animState->getCurrent(0);
    return track ? track->getAnimationTime() : 0.0f;
}

float spine_get_duration(SpineHandle handle) {
    if (!handle) return 0.0f;
    TrackEntry* track = ((SpineInstance*)handle)->animState->getCurrent(0);
    return track ? track->getAnimationEnd() : 0.0f;
}

void spine_set_loop(SpineHandle handle, bool loop) {
    if (!handle) return;
    TrackEntry* track = ((SpineInstance*)handle)->animState->getCurrent(0);
    if (track) track->setLoop(loop);
}

bool spine_get_loop(SpineHandle handle) {
    if (!handle) return false;
    TrackEntry* track = ((SpineInstance*)handle)->animState->getCurrent(0);
    return track ? track->getLoop() : false;
}

void spine_set_timescale(SpineHandle handle, float timescale) {
    if (handle) ((SpineInstance*)handle)->animState->setTimeScale(timescale);
}

float spine_get_timescale(SpineHandle handle) {
    return handle ? ((SpineInstance*)handle)->animState->getTimeScale() : 1.0f;
}

void spine_set_position(SpineHandle handle, float x, float y) {
    if (!handle) return;
    SpineInstance* inst = (SpineInstance*)handle;
    inst->skeleton->setX(x);
    inst->skeleton->setY(y);
}

void spine_get_position(SpineHandle handle, float* out_x, float* out_y) {
    if (!handle) return;
    SpineInstance* inst = (SpineInstance*)handle;
    if (out_x) *out_x = inst->skeleton->getX();
    if (out_y) *out_y = inst->skeleton->getY();
}

void spine_set_scale(SpineHandle handle, float scale_x, float scale_y) {
    if (!handle) return;
    SpineInstance* inst = (SpineInstance*)handle;
    inst->skeleton->setScaleX(scale_x);
    inst->skeleton->setScaleY(scale_y);
}

void spine_update(SpineHandle handle, float dt) {
    if (!handle) return;
    SpineInstance* inst = (SpineInstance*)handle;
    inst->skeleton->update(dt);
    inst->animState->update(dt);
    inst->animState->apply(*inst->skeleton);
    inst->skeleton->updateWorldTransform();
}

void spine_apply_setup_pose(SpineHandle handle) {
    if (!handle) return;
    SpineInstance* inst = (SpineInstance*)handle;
    inst->skeleton->setToSetupPose();
    inst->skeleton->updateWorldTransform();
}

bool spine_compute_max_bounds(SpineHandle handle, const char* anim_name, float precision,
                             float* out_x, float* out_y, float* out_w, float* out_h) {
    if (!handle || !anim_name) return false;
    SpineInstance* inst = (SpineInstance*)handle;
    Animation* anim = inst->skeletonData->findAnimation(String(anim_name));
    if (!anim) return false;

    if (precision <= 0.0f) precision = 0.016f;

    inst->skeleton->setX(0.0f);
    inst->skeleton->setY(0.0f);
    inst->animState->setAnimation(0, anim, false);
    spine_update(handle, 0.0f);

    float duration = anim->getDuration();
    float min_x = 1e9f, min_y = 1e9f, max_x = -1e9f, max_y = -1e9f;
    bool has_bounds = false;

    for (float t = 0.0f; t <= duration + (precision * 0.5f); t += precision) {
        const SpineRenderData* rd = spine_generate_mesh(handle, false);
        if (rd && rd->vertex_count > 0) {
            float vx0 = 1e9f, vy0 = 1e9f, vx1 = -1e9f, vy1 = -1e9f;
            for (uint32_t i = 0; i < rd->vertex_count; ++i) {
                vx0 = std::min(vx0, rd->vertices[i].x);
                vy0 = std::min(vy0, rd->vertices[i].y);
                vx1 = std::max(vx1, rd->vertices[i].x);
                vy1 = std::max(vy1, rd->vertices[i].y);
            }
            float cur_w = vx1 - vx0, cur_h = vy1 - vy0;
            if (cur_w > 0.0f && cur_h > 0.0f) {
                min_x = has_bounds ? std::min(min_x, vx0) : vx0;
                min_y = has_bounds ? std::min(min_y, vy0) : vy0;
                max_x = has_bounds ? std::max(max_x, vx1) : vx1;
                max_y = has_bounds ? std::max(max_y, vy1) : vy1;
                has_bounds = true;
            }
        }
        spine_update(handle, precision);
    }

    if (!has_bounds) { min_x = 0; min_y = 0; max_x = 100; max_y = 100; }
    if (out_x) *out_x = min_x;
    if (out_y) *out_y = min_y;
    if (out_w) *out_w = std::max(1.0f, max_x - min_x);
    if (out_h) *out_h = std::max(1.0f, max_y - min_y);
    return true;
}

static inline uint32_t pack_color(float r, float g, float b, float a, bool pma) {
    if (pma) { r *= a; g *= a; b *= a; }
    uint32_t ir = (uint32_t)(std::max(0.0f, std::min(1.0f, r)) * 255.0f);
    uint32_t ig = (uint32_t)(std::max(0.0f, std::min(1.0f, g)) * 255.0f);
    uint32_t ib = (uint32_t)(std::max(0.0f, std::min(1.0f, b)) * 255.0f);
    uint32_t ia = (uint32_t)(std::max(0.0f, std::min(1.0f, a)) * 255.0f);
    return (ia << 24) | (ib << 16) | (ig << 8) | ir;
}

const SpineRenderData* spine_generate_mesh(SpineHandle handle, bool pma) {
    if (!handle) return nullptr;
    SpineInstance* inst = (SpineInstance*)handle;

    inst->vertices.clear();
    inst->indices.clear();
    inst->batches.clear();

    Color& skeletonColor = inst->skeleton->getColor();
    if (skeletonColor.a == 0.0f) {
        inst->renderData.vertex_count = 0;
        inst->renderData.index_count = 0;
        inst->renderData.batch_count = 0;
        return &inst->renderData;
    }

    int current_texture_index = -1;
    int current_blend_mode = -1;
    uint32_t batch_start_index = 0;

    auto flush_batch = [&]() {
        uint32_t count = (uint32_t)inst->indices.size() - batch_start_index;
        if (count > 0 && current_texture_index >= 0) {
            inst->batches.push_back({current_texture_index, current_blend_mode, batch_start_index, count});
            batch_start_index = (uint32_t)inst->indices.size();
        }
    };

    Vector<Slot*>& drawOrder = inst->skeleton->getDrawOrder();

    for (size_t i = 0; i < drawOrder.size(); ++i) {
        Slot* slot = drawOrder[i];
        if (!slot) continue;

        Attachment* attachment = slot->getAttachment();
        if (!attachment || slot->getColor().a == 0.0f) {
            inst->clipper.clipEnd(*slot);
            continue;
        }

        int blend_mode = (int)slot->getData().getBlendMode();

        if (attachment->getRTTI().isExactly(ClippingAttachment::rtti)) {
            inst->clipper.clipStart(*slot, (ClippingAttachment*)attachment);
            continue;
        }

        int texture_index = -1;
        float* worldVertices = nullptr;
        size_t worldVerticesCount = 0;
        float* uvs = nullptr;
        unsigned short* triangles = nullptr;
        size_t trianglesCount = 0;
        Color attachmentColor(1, 1, 1, 1);

        if (attachment->getRTTI().isExactly(RegionAttachment::rtti)) {
            RegionAttachment* region = (RegionAttachment*)attachment;
            attachmentColor = region->getColor();
            if (attachmentColor.a == 0.0f) {
                inst->clipper.clipEnd(*slot);
                continue;
            }

            inst->worldVerticesTemp.setSize(8, 0);
            region->computeWorldVertices(slot->getBone(), inst->worldVerticesTemp, 0, 2);
            worldVertices = inst->worldVerticesTemp.buffer();
            worldVerticesCount = 8;
            uvs = region->getUVs().buffer();

            static unsigned short quadTriangles[6] = { 0, 1, 2, 2, 3, 0 };
            triangles = quadTriangles;
            trianglesCount = 6;

            AtlasRegion* aReg = (AtlasRegion*)region->getRendererObject();
            if (aReg && aReg->page) {
                texture_index = (int)(uintptr_t)aReg->page->getRendererObject() - 1;
            }
        } else if (attachment->getRTTI().isExactly(MeshAttachment::rtti)) {
            MeshAttachment* mesh = (MeshAttachment*)attachment;
            attachmentColor = mesh->getColor();
            if (attachmentColor.a == 0.0f) {
                inst->clipper.clipEnd(*slot);
                continue;
            }

            size_t numVerts = mesh->getWorldVerticesLength();
            inst->worldVerticesTemp.setSize(numVerts, 0);
            mesh->computeWorldVertices(*slot, 0, numVerts, inst->worldVerticesTemp.buffer(), 0, 2);

            worldVertices = inst->worldVerticesTemp.buffer();
            worldVerticesCount = numVerts;
            uvs = mesh->getUVs().buffer();
            triangles = mesh->getTriangles().buffer();
            trianglesCount = mesh->getTriangles().size();

            AtlasRegion* aReg = (AtlasRegion*)mesh->getRendererObject();
            if (aReg && aReg->page) {
                texture_index = (int)(uintptr_t)aReg->page->getRendererObject() - 1;
            }
        } else {
            inst->clipper.clipEnd(*slot);
            continue;
        }

        if (texture_index < 0 || !worldVertices || trianglesCount == 0) {
            inst->clipper.clipEnd(*slot);
            continue;
        }

        if (texture_index != current_texture_index || blend_mode != current_blend_mode) {
            flush_batch();
            current_texture_index = texture_index;
            current_blend_mode = blend_mode;
        }

        Color& slotColor = slot->getColor();
        float r = skeletonColor.r * slotColor.r * attachmentColor.r;
        float g = skeletonColor.g * slotColor.g * attachmentColor.g;
        float b = skeletonColor.b * slotColor.b * attachmentColor.b;
        float a = skeletonColor.a * slotColor.a * attachmentColor.a;
        uint32_t finalColor = pack_color(r, g, b, a, pma);

        if (inst->clipper.isClipping()) {
            inst->clipper.clipTriangles(worldVertices, triangles, trianglesCount, uvs, 2);
            float* clippedVerts = inst->clipper.getClippedVertices().buffer();
            size_t clippedVertsCount = inst->clipper.getClippedVertices().size();
            float* clippedUVs = inst->clipper.getClippedUVs().buffer();
            unsigned short* clippedTris = inst->clipper.getClippedTriangles().buffer();
            size_t clippedTrisCount = inst->clipper.getClippedTriangles().size();

            uint32_t baseVertex = (uint32_t)inst->vertices.size();
            size_t numPoints = clippedVertsCount / 2;
            for (size_t p = 0; p < numPoints; ++p) {
                inst->vertices.push_back({clippedVerts[p * 2], clippedVerts[p * 2 + 1], clippedUVs[p * 2], clippedUVs[p * 2 + 1], finalColor});
            }
            for (size_t t = 0; t < clippedTrisCount; ++t) {
                inst->indices.push_back((uint16_t)(baseVertex + clippedTris[t]));
            }
        } else {
            uint32_t baseVertex = (uint32_t)inst->vertices.size();
            size_t numPoints = worldVerticesCount / 2;
            for (size_t p = 0; p < numPoints; ++p) {
                inst->vertices.push_back({worldVertices[p * 2], worldVertices[p * 2 + 1], uvs[p * 2], uvs[p * 2 + 1], finalColor});
            }
            for (size_t t = 0; t < trianglesCount; ++t) {
                inst->indices.push_back((uint16_t)(baseVertex + triangles[t]));
            }
        }

        inst->clipper.clipEnd(*slot);
    }

    inst->clipper.clipEnd();
    flush_batch();

    inst->renderData.vertices = inst->vertices.data();
    inst->renderData.vertex_count = (uint32_t)inst->vertices.size();
    inst->renderData.indices = inst->indices.data();
    inst->renderData.index_count = (uint32_t)inst->indices.size();
    inst->renderData.batches = inst->batches.data();
    inst->renderData.batch_count = (uint32_t)inst->batches.size();

    return &inst->renderData;
}

} // extern "C"
