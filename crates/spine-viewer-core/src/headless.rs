//! 창 없이(숨김 창) 스켈레톤을 PNG/시트로 내보내는 헤드리스 CLI.
//! AI/MCP가 `sv-cli.exe` 를 호출해 일괄 렌더할 때 쓴다. 결과는 stdout에 JSON 한 덩어리로 출력한다.
//!
//! 사용법:
//!   sv-cli info  <스켈레톤|폴더> [--atlas <파일>]
//!   sv-cli frame <스켈레톤|폴더> --out <폴더|파일.png> [--animation A] [--time 초|largest] [--skin 이름|all]
//!                [--scale 0.5] [--no-crop] [--keep-shadow] [--no-pma] [--atlas <파일>]
//!   sv-cli sheet <스켈레톤|폴더> --out <폴더> [--animation A] [--skin 이름|all] [--fps 24] [--cols 8]
//!                [--scale 1.0] [--no-sheet-crop] [--keep-shadow] [--no-json] [--no-pma] [--atlas <파일>]
//!
//! 폴더를 주면 하위 .skel/.json 을 전부 처리한다. 이때 스켈레톤마다 자기 자신을 자식 프로세스로 다시 실행해서
//! 처리한다 (Spine 런타임의 어서션/크래시가 나도 해당 파일만 `skipped` 로 기록하고 나머지는 계속 진행).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use eframe::{egui, glow};
use serde_json::{json, Value};

use crate::export::{render_pose, render_sheet, SheetOptions};
use crate::recorder::Recorder;
use crate::renderer::GlRenderer;
use crate::spine_wrapper::SpineSkeleton;

const BOOL_FLAGS: &[&str] = &["no-crop", "no-pma", "no-sheet-crop", "no-json", "keep-shadow"];

#[derive(Clone)]
struct Opts {
    positional: Vec<String>,
    flags: HashMap<String, String>,
    switches: HashSet<String>,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut o = Opts { positional: vec![], flags: HashMap::new(), switches: HashSet::new() };
        let mut i = 0;
        while i < args.len() {
            if let Some(name) = args[i].strip_prefix("--") {
                if BOOL_FLAGS.contains(&name) {
                    o.switches.insert(name.to_string());
                } else {
                    i += 1;
                    let v = args.get(i).ok_or_else(|| format!("--{} 값이 없습니다.", name))?;
                    o.flags.insert(name.to_string(), v.clone());
                }
            } else {
                o.positional.push(args[i].clone());
            }
            i += 1;
        }
        Ok(o)
    }

    fn num(&self, key: &str, default: f32) -> Result<f32, String> {
        match self.flags.get(key) {
            None => Ok(default),
            Some(v) => v.parse::<f32>().map_err(|_| format!("--{} 는 숫자여야 합니다: {}", key, v)),
        }
    }

    /// 자식 프로세스에 넘길 인자 (입력 경로만 바꾸고 나머지 옵션은 그대로).
    fn child_args(&self, cmd: &str, input: &Path, root: &Path) -> Vec<String> {
        let mut a = vec![cmd.to_string(), input.to_string_lossy().into_owned()];
        for (k, v) in &self.flags {
            if k != "root" {
                a.push(format!("--{}", k));
                a.push(v.clone());
            }
        }
        for s in &self.switches {
            a.push(format!("--{}", s));
        }
        a.push("--root".to_string());
        a.push(root.to_string_lossy().into_owned());
        a
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn ext_lower(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn stem_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 파일이면 그대로, 폴더면 하위의 .skel/.json 을 재귀 수집한다 (이름순).
fn collect_inputs(root: &Path) -> Result<Vec<PathBuf>, String> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if !root.is_dir() {
        return Err(format!("경로가 없습니다: {}", root.display()));
    }
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(ext_lower(&p).as_str(), "skel" | "json") {
                found.push(p);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// 아틀라스 텍스트에서 리전 이름을 뽑는다 (3.8 형식: 페이지 줄, `key: value` 줄, 이름 줄 + 들여쓴 속성).
fn atlas_region_names(path: &Path) -> Vec<String> {
    let text = match std::fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(_) => return vec![],
    };
    let mut names = Vec::new();
    let mut expect_page = true;
    for line in text.lines() {
        let l = line.trim_end();
        if l.is_empty() {
            expect_page = true;
        } else if expect_page {
            expect_page = false; // 페이지 이미지 파일 줄
        } else if !l.starts_with(char::is_whitespace) && !l.contains(':') {
            names.push(l.to_string());
        }
    }
    names
}

/// 스켈레톤 파일에서 출력 가능한 문자열 조각(길이 접두 바이트가 붙은 경우까지)을 모은다.
fn skeleton_strings(data: &Path) -> HashSet<String> {
    let bytes = std::fs::read(data).unwrap_or_default();
    let mut set = HashSet::new();
    let mut run: Vec<u8> = Vec::new();
    let flush = |run: &mut Vec<u8>, set: &mut HashSet<String>| {
        if run.len() >= 2 {
            let s = String::from_utf8_lossy(run).into_owned();
            // 문자열 앞의 길이 바이트가 출력 가능한 문자일 수 있으므로 첫 글자를 뗀 것도 넣는다.
            if let Some((_, rest)) = s.char_indices().nth(1).map(|(i, _)| s.split_at(i)) {
                set.insert(rest.to_string());
            }
            set.insert(s);
        }
        run.clear();
    };
    for &b in &bytes {
        // UTF-8 연속 바이트(한글 등)도 문자열의 일부로 취급한다.
        if (0x20..0x7f).contains(&b) && b != b'"' || b >= 0x80 {
            run.push(b);
        } else {
            flush(&mut run, &mut set);
        }
    }
    flush(&mut run, &mut set);
    set
}

/// 같은 폴더의 아틀라스 후보를 "스켈레톤이 쓰는 리전 이름과 얼마나 겹치는지"로 점수 매겨 정렬한다.
/// 겹침이 0인 아틀라스는 시도하지 않는다 (엉뚱한 짝으로 로드하면 Spine 런타임이 어서션으로 죽는다).
fn atlas_candidates(data: &Path) -> Vec<PathBuf> {
    let dir = data.parent().unwrap_or_else(|| Path::new("."));
    let stem = stem_of(data);
    let strings = skeleton_strings(data);
    // 같은 폴더의 다른 스켈레톤 이름. 그 이름의 아틀라스(예: 0107_air_double.atlas)는 그 스켈레톤 전용일 가능성이 높아
    // 리전 이름이 겹쳐도 뒤로 미룬다 (공용 아틀라스 s01_atlas 같은 것이 우선).
    let other_skeletons: HashSet<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| matches!(ext_lower(p).as_str(), "skel" | "json") && stem_of(p) != stem)
                .map(|p| stem_of(&p))
                .collect()
        })
        .unwrap_or_default();
    let mut scored: Vec<(usize, u8, PathBuf)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| ext_lower(p) == "atlas")
                .filter_map(|p| {
                    let regions = atlas_region_names(&p);
                    let score = regions.iter().filter(|r| strings.contains(*r)).count();
                    if score == 0 {
                        return None;
                    }
                    let name_rank = if stem_of(&p) == stem {
                        0
                    } else if other_skeletons.contains(&stem_of(&p)) {
                        3
                    } else if stem.starts_with(&stem_of(&p)) {
                        1
                    } else {
                        2
                    };
                    Some((score, name_rank, p))
                })
                .collect()
        })
        .unwrap_or_default();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    scored.into_iter().map(|(_, _, p)| p).collect()
}

/// 스켈레톤을 로드한다. 아틀라스가 지정되지 않으면 리전 겹침이 큰 후보부터 시도해 처음 성공한 짝을 쓴다.
fn load_skeleton(data: &Path, atlas: Option<&Path>) -> Result<(SpineSkeleton, PathBuf), String> {
    let is_json = ext_lower(data) == "json";
    if let Some(a) = atlas {
        return SpineSkeleton::load(a, data, is_json).map(|s| (s, a.to_path_buf()));
    }
    let candidates = atlas_candidates(data);
    if candidates.is_empty() {
        return Err("리전 이름이 겹치는 .atlas 를 같은 폴더에서 찾지 못했습니다 (--atlas 로 직접 지정하세요).".to_string());
    }
    let mut last_err = String::new();
    for c in &candidates {
        match SpineSkeleton::load(c, data, is_json) {
            Ok(s) => return Ok((s, c.clone())),
            Err(e) => last_err = e,
        }
    }
    Err(format!("맞는 아틀라스를 찾지 못했습니다 (후보 {}개, 마지막 오류: {})", candidates.len(), last_err))
}

fn animations_with_duration(sk: &SpineSkeleton) -> Vec<(String, f32)> {
    sk.get_animations()
        .into_iter()
        .map(|a| {
            sk.set_animation(&a, false);
            let d = sk.get_duration();
            (a, d)
        })
        .collect()
}

fn pick_animation(sk: &SpineSkeleton, requested: Option<&str>) -> Result<String, String> {
    let anims = sk.get_animations();
    match requested {
        Some(name) => {
            if anims.iter().any(|a| a == name) {
                Ok(name.to_string())
            } else {
                Err(format!("애니메이션 '{}' 없음 (있는 것: {})", name, anims.join(", ")))
            }
        }
        None => {
            if anims.iter().any(|a| a == "standby") {
                Ok("standby".to_string())
            } else {
                anims.first().cloned().ok_or_else(|| "애니메이션이 없습니다.".to_string())
            }
        }
    }
}

/// `--skin` 값을 (스킨 이름 또는 None=기본) 목록으로 푼다.
fn skin_variants(sk: &SpineSkeleton, requested: Option<&str>) -> Result<Vec<Option<String>>, String> {
    let skins = sk.get_skins();
    match requested {
        None => Ok(vec![None]),
        Some("all") => {
            let extra: Vec<Option<String>> = skins.into_iter().filter(|s| s != "default").map(Some).collect();
            Ok(if extra.is_empty() { vec![None] } else { extra })
        }
        Some(name) => {
            if skins.iter().any(|s| s == name) {
                Ok(vec![Some(name.to_string())])
            } else {
                Err(format!("스킨 '{}' 없음 (있는 것: {})", name, skins.join(", ")))
            }
        }
    }
}

fn load_page_textures(gl: &glow::Context, renderer: &mut GlRenderer, sk: &SpineSkeleton) -> Result<(), String> {
    let paths: Vec<String> = (0..sk.get_page_count()).map(|i| sk.get_page_path(i)).collect();
    unsafe { renderer.clear_textures(gl) };
    let sizes = unsafe { renderer.load_textures(gl, &paths)? };
    for (i, (w, h)) in sizes.iter().enumerate() {
        sk.set_page_texture_size(i as i32, *w, *h);
    }
    Ok(())
}

/// 스킨/애니메이션을 정하고 최대 바운드에 맞춰 FBO와 위치를 맞춘다 (GUI의 `update_render_texture_bounds`와 같은 순서).
fn set_view(
    gl: &glow::Context,
    renderer: &mut GlRenderer,
    sk: &SpineSkeleton,
    anim: &str,
    skin: Option<&str>,
) -> Result<(), String> {
    if let Some(s) = skin {
        if !sk.set_skin(s) {
            return Err(format!("스킨 '{}' 적용 실패", s));
        }
    }
    let b = sk.compute_max_bounds(anim, 0.016)?;
    let (w, h) = ((b.width.ceil() as i32).max(1), (b.height.ceil() as i32).max(1));
    if w > 16384 || h > 16384 {
        return Err(format!("바운드가 너무 큽니다: {}x{}", w, h));
    }
    sk.set_position(-b.x, -b.y);
    sk.set_animation(anim, false);
    sk.set_time(0.0);
    unsafe { renderer.resize_render_texture(gl, w, h) }
}

struct Common {
    inputs: Vec<PathBuf>,
    /// 폴더 입력이면 그 폴더 (출력 하위 구조 계산용). 자식 프로세스는 `--root` 로 받는다.
    root: PathBuf,
    out: PathBuf,
    atlas: Option<PathBuf>,
    animation: Option<String>,
    skin: Option<String>,
    pma: bool,
}

impl Common {
    fn from(o: &Opts, need_out: bool) -> Result<Self, String> {
        let input = o.positional.get(1).ok_or("입력 경로(스켈레톤 파일 또는 폴더)가 필요합니다.")?;
        let input = PathBuf::from(input);
        let inputs = collect_inputs(&input)?;
        if inputs.is_empty() {
            return Err("스켈레톤(.skel/.json)을 찾지 못했습니다.".to_string());
        }
        let root = o.flags.get("root").map(PathBuf::from).unwrap_or_else(|| input.clone());
        let out = match o.flags.get("out") {
            Some(v) => PathBuf::from(v),
            None if need_out => return Err("--out 이 필요합니다.".to_string()),
            None => PathBuf::new(),
        };
        Ok(Self {
            inputs,
            root,
            out,
            atlas: o.flags.get("atlas").map(PathBuf::from),
            animation: o.flags.get("animation").cloned(),
            skin: o.flags.get("skin").cloned(),
            pma: !o.switches.contains("no-pma"),
        })
    }

    /// 입력이 폴더면 하위 구조를 출력 폴더에 그대로 따라간다.
    fn out_dir_for(&self, data: &Path) -> PathBuf {
        if self.root.is_dir() {
            if let Some(rel) = data.parent().and_then(|p| p.strip_prefix(&self.root).ok()) {
                return self.out.join(rel);
            }
        }
        self.out.clone()
    }
}

fn skipped(input: &Path, reason: &str) -> Value {
    json!({ "input": path_str(input), "reason": reason })
}

fn envelope(results: Vec<Value>, skipped: Vec<Value>) -> Value {
    json!({ "ok": true, "count": results.len(), "results": results, "skipped": skipped })
}

fn error_value(msg: &str) -> Value {
    json!({ "ok": false, "error": msg })
}

fn with_skin_prefix(skin: &Option<String>, msg: String) -> String {
    match skin {
        Some(s) => format!("[스킨 {}] {}", s, msg),
        None => msg,
    }
}

fn info_one(data: &Path, atlas: Option<&Path>) -> Result<Value, String> {
    let (sk, atlas) = load_skeleton(data, atlas)?;
    let anims: Vec<Value> = animations_with_duration(&sk)
        .iter()
        .map(|(n, d)| json!({ "name": n, "duration": (*d as f64 * 10000.0).round() / 10000.0 }))
        .collect();
    Ok(json!({
        "input": path_str(data),
        "atlas": path_str(&atlas),
        "pages": sk.get_page_count(),
        "skins": sk.get_skins(),
        "animations": anims,
    }))
}

fn run_info_inputs(c: &Common) -> Value {
    let mut results = Vec::new();
    let mut skips = Vec::new();
    for data in &c.inputs {
        match info_one(data, c.atlas.as_deref()) {
            Ok(v) => results.push(v),
            Err(e) => skips.push(skipped(data, &e)),
        }
    }
    envelope(results, skips)
}

fn run_frame(gl: &glow::Context, o: &Opts) -> Value {
    let c = match Common::from(o, true) {
        Ok(c) => c,
        Err(e) => return error_value(&e),
    };
    // --time 은 초 단위 숫자, 또는 `largest` (애니메이션에서 본체가 가장 큰 프레임을 고른다)
    let largest = o.flags.get("time").map(|s| s == "largest").unwrap_or(false);
    let time_explicit = o.flags.contains_key("time");
    let time = if largest {
        0.0
    } else {
        match o.num("time", 0.0) {
            Ok(v) => v.max(0.0),
            Err(e) => return error_value(&e),
        }
    };
    let trim = !o.switches.contains("keep-shadow");
    let scale = match o.num("scale", 1.0) {
        Ok(v) => v.clamp(0.01, 1.0),
        Err(e) => return error_value(&e),
    };
    let crop = !o.switches.contains("no-crop");
    let mut renderer = match unsafe { GlRenderer::new(gl) } {
        Ok(r) => r,
        Err(e) => return error_value(&format!("렌더러 초기화 실패: {}", e)),
    };

    let mut results = Vec::new();
    let mut skips = Vec::new();
    let single_png = c.out.extension().map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false);

    for data in &c.inputs {
        let (sk, atlas) = match load_skeleton(data, c.atlas.as_deref()) {
            Ok(v) => v,
            Err(e) => {
                skips.push(skipped(data, &e));
                continue;
            }
        };
        let prep = pick_animation(&sk, c.animation.as_deref())
            .and_then(|a| skin_variants(&sk, c.skin.as_deref()).map(|s| (a, s)))
            .and_then(|v| load_page_textures(gl, &mut renderer, &sk).map(|_| v));
        let (anim, skins) = match prep {
            Ok(v) => v,
            Err(e) => {
                skips.push(skipped(data, &e));
                continue;
            }
        };
        let stem = Recorder::sanitize(&stem_of(data));

        for skin in &skins {
            let one = (|| -> Result<(PathBuf, u32, u32, f32, u32), String> {
                set_view(gl, &mut renderer, &sk, &anim, skin.as_deref())?;
                let dur = sk.get_duration();
                let (fw, fh) = (renderer.fbo_width as u32, renderer.fbo_height as u32);
                // 본체가 가장 큰 시점을 찾는다 (최대 150개 시점 샘플링).
                let find_largest = || -> (Vec<u8>, f32) {
                    let step = (dur / 150.0).max(1.0 / 30.0);
                    let mut best: Option<(u64, f32, Vec<u8>)> = None;
                    let mut t = 0.0f32;
                    loop {
                        let tt = t.min(dur);
                        let px = render_pose(gl, &renderer, &sk, tt, c.pma);
                        let area = Recorder::analyze_content(fw, fh, &px, trim).map(|i| i.area).unwrap_or(0);
                        if best.as_ref().map_or(true, |b| area > b.0) {
                            best = Some((area, tt, px));
                        }
                        if t >= dur {
                            break;
                        }
                        t += step;
                    }
                    let (_, tt, px) = best.unwrap();
                    (px, tt)
                };
                let (px, used_time) = if largest {
                    find_largest()
                } else {
                    let tt = time.min(dur);
                    let px = render_pose(gl, &renderer, &sk, tt, c.pma);
                    // 시간을 따로 지정하지 않았는데 그 시점이 비어 있으면(예: in_/out_ 연출 첫 프레임) 가장 큰 시점으로 대신한다.
                    if !time_explicit && Recorder::analyze_content(fw, fh, &px, trim).is_none() {
                        find_largest()
                    } else {
                        (px, tt)
                    }
                };
                let mut trimmed = 0;
                let (mut w, mut h, mut buf) = if crop {
                    let info = Recorder::analyze_content(fw, fh, &px, trim)
                        .ok_or("이 시점에는 보이는 픽셀이 없습니다.")?;
                    trimmed = info.trimmed;
                    let r = info.rect;
                    (r.2 - r.0 + 1, r.3 - r.1 + 1, Recorder::crop_rect(fw, &px, r))
                } else {
                    (fw, fh, px)
                };
                if scale < 1.0 {
                    let img = image::RgbaImage::from_raw(w, h, buf).ok_or("프레임 버퍼 크기 오류")?;
                    let (nw, nh) = (
                        ((w as f32 * scale).round() as u32).max(1),
                        ((h as f32 * scale).round() as u32).max(1),
                    );
                    buf = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3).into_raw();
                    w = nw;
                    h = nh;
                }
                let path = if single_png && c.inputs.len() == 1 && skins.len() == 1 {
                    c.out.clone()
                } else {
                    let name = match skin {
                        Some(s) => format!("{}__{}.png", stem, Recorder::sanitize(s)),
                        None => format!("{}.png", stem),
                    };
                    c.out_dir_for(data).join(name)
                };
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("폴더 생성 실패: {}", e))?;
                }
                image::save_buffer(&path, &buf, w, h, image::ExtendedColorType::Rgba8)
                    .map_err(|e| format!("저장 실패: {}", e))?;
                Ok((path, w, h, used_time, trimmed))
            })();
            match one {
                Ok((path, w, h, used_time, trimmed)) => results.push(json!({
                    "input": path_str(data),
                    "atlas": path_str(&atlas),
                    "animation": anim,
                    "skin": skin,
                    "time": (used_time as f64 * 1000.0).round() / 1000.0,
                    "output": path_str(&path),
                    "width": w,
                    "height": h,
                    "shadowTrimmed": trimmed,
                })),
                Err(e) => skips.push(skipped(data, &with_skin_prefix(skin, e))),
            }
        }
    }
    unsafe { renderer.destroy(gl) };
    envelope(results, skips)
}

fn run_sheet(gl: &glow::Context, o: &Opts) -> Value {
    let c = match Common::from(o, true) {
        Ok(c) => c,
        Err(e) => return error_value(&e),
    };
    let (fps, cols, scale) = match (o.num("fps", 24.0), o.num("cols", 8.0), o.num("scale", 1.0)) {
        (Ok(a), Ok(b), Ok(s)) => (a, b.max(1.0) as u32, s),
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => return error_value(&e),
    };
    let crop = !o.switches.contains("no-sheet-crop");
    let write_json = !o.switches.contains("no-json");
    let mut renderer = match unsafe { GlRenderer::new(gl) } {
        Ok(r) => r,
        Err(e) => return error_value(&format!("렌더러 초기화 실패: {}", e)),
    };

    let mut results = Vec::new();
    let mut skips = Vec::new();
    for data in &c.inputs {
        let (sk, atlas) = match load_skeleton(data, c.atlas.as_deref()) {
            Ok(v) => v,
            Err(e) => {
                skips.push(skipped(data, &e));
                continue;
            }
        };
        let prep = pick_animation(&sk, c.animation.as_deref())
            .and_then(|a| skin_variants(&sk, c.skin.as_deref()).map(|s| (a, s)))
            .and_then(|v| load_page_textures(gl, &mut renderer, &sk).map(|_| v));
        let (anim, skins) = match prep {
            Ok(v) => v,
            Err(e) => {
                skips.push(skipped(data, &e));
                continue;
            }
        };
        let stem = Recorder::sanitize(&stem_of(data));

        for skin in &skins {
            let one = (|| -> Result<Value, String> {
                set_view(gl, &mut renderer, &sk, &anim, skin.as_deref())?;
                let opts = SheetOptions { fps, cols, scale, crop, trim_shadow: !o.switches.contains("keep-shadow"), pma: c.pma };
                let (sheet, info) = render_sheet(gl, &renderer, &sk, &opts, 0.0)?;
                let base = match skin {
                    Some(s) => format!("{}__{}_{}_sheet", stem, Recorder::sanitize(s), Recorder::sanitize(&anim)),
                    None => format!("{}_{}_sheet", stem, Recorder::sanitize(&anim)),
                };
                let dir = c.out_dir_for(data);
                std::fs::create_dir_all(&dir).map_err(|e| format!("폴더 생성 실패: {}", e))?;
                let png = dir.join(format!("{}.png", base));
                sheet.save(&png)?;
                let json_path = dir.join(format!("{}.json", base));
                if write_json {
                    std::fs::write(&json_path, info.json(&format!("{}.png", base)))
                        .map_err(|e| format!("json 저장 실패: {}", e))?;
                }
                Ok(json!({
                    "input": path_str(data),
                    "atlas": path_str(&atlas),
                    "animation": anim,
                    "skin": skin,
                    "output": path_str(&png),
                    "metadata": if write_json { Value::String(path_str(&json_path)) } else { Value::Null },
                    "sheetWidth": sheet.width(),
                    "sheetHeight": sheet.height(),
                    "frameWidth": info.cell_w,
                    "frameHeight": info.cell_h,
                    "columns": info.cols,
                    "rows": info.rows,
                    "frameCount": info.count,
                    "fps": info.fps,
                    "duration": info.duration,
                    "scale": info.scale,
                }))
            })();
            match one {
                Ok(v) => results.push(v),
                Err(e) => skips.push(skipped(data, &with_skin_prefix(skin, e))),
            }
        }
    }
    unsafe { renderer.destroy(gl) };
    envelope(results, skips)
}

type JobFn = Box<dyn FnOnce(&glow::Context) -> Value>;

/// 숨김 창을 띄워 GL 컨텍스트를 얻고, 첫 프레임에서 작업을 실행한 뒤 창을 닫는다.
struct HeadlessApp {
    job: Option<JobFn>,
    out: Arc<Mutex<Option<Value>>>,
}

impl eframe::App for HeadlessApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(job) = self.job.take() {
            let res = match frame.gl() {
                Some(gl) => job(gl),
                None => error_value("GL 컨텍스트를 얻지 못했습니다."),
            };
            *self.out.lock().unwrap() = Some(res);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn run_with_gl(job: JobFn) -> Value {
    let out = Arc::new(Mutex::new(None));
    let app = HeadlessApp { job: Some(job), out: out.clone() };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_inner_size([64.0, 64.0])
            .with_title("sv-cli"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let run = eframe::run_native("sv-cli", options, Box::new(move |_cc| Ok(Box::new(app))));
    let result = out.lock().unwrap().take();
    match (result, run) {
        (Some(r), _) => r,
        (None, Err(e)) => error_value(&format!("창/GL 초기화 실패: {}", e)),
        (None, Ok(())) => error_value("작업이 실행되지 않았습니다 (숨김 창에서 프레임이 그려지지 않음)."),
    }
}

/// 폴더 입력: 스켈레톤마다 자식 프로세스로 처리해 크래시를 격리하고 결과를 합친다.
fn supervise(cmd: &str, o: &Opts, c: &Common) -> Value {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return error_value(&format!("실행 파일 경로 오류: {}", e)),
    };
    let mut results: Vec<Value> = Vec::new();
    let mut skips: Vec<Value> = Vec::new();
    for data in &c.inputs {
        let output = Command::new(&exe).args(o.child_args(cmd, data, &c.root)).output();
        match output {
            Err(e) => skips.push(skipped(data, &format!("자식 프로세스 실행 실패: {}", e))),
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                // 자식은 마지막 줄에 JSON 한 덩어리를 출력한다.
                let parsed = stdout.lines().rev().find_map(|l| serde_json::from_str::<Value>(l.trim()).ok());
                match parsed {
                    Some(v) if v["ok"] == true => {
                        results.extend(v["results"].as_array().cloned().unwrap_or_default());
                        skips.extend(v["skipped"].as_array().cloned().unwrap_or_default());
                    }
                    Some(v) => skips.push(skipped(data, v["error"].as_str().unwrap_or("알 수 없는 오류"))),
                    None => {
                        let err = String::from_utf8_lossy(&out.stderr);
                        let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("출력 없음");
                        skips.push(skipped(data, &format!("처리 중 비정상 종료: {}", first.trim())));
                    }
                }
            }
        }
    }
    envelope(results, skips)
}

const USAGE: &str = "사용법:\n  sv-cli info  <스켈레톤|폴더> [--atlas 파일]\n  sv-cli frame <스켈레톤|폴더> --out <폴더|파일.png> [--animation A] [--time 초|largest] [--skin 이름|all] [--scale 0.5] [--no-crop] [--keep-shadow] [--no-pma] [--atlas 파일]\n  sv-cli sheet <스켈레톤|폴더> --out <폴더> [--animation A] [--skin 이름|all] [--fps 24] [--cols 8] [--scale 1.0] [--no-sheet-crop] [--keep-shadow] [--no-json] [--no-pma] [--atlas 파일]";

/// `args` 는 `std::env::args()` 전체(0번은 실행 파일). 종료 코드를 돌려준다.
pub fn run(args: Vec<String>) -> i32 {
    let rest: Vec<String> = args.into_iter().skip(1).collect();
    let cmd = rest.first().cloned().unwrap_or_default();
    let opts = match Opts::parse(&rest) {
        Ok(o) => o,
        Err(e) => {
            println!("{}", error_value(&e));
            return 1;
        }
    };
    if !matches!(cmd.as_str(), "info" | "frame" | "sheet") {
        eprintln!("{}", USAGE);
        println!("{}", error_value("알 수 없는 명령입니다. frame / sheet / info 중 하나를 쓰세요."));
        return 2;
    }

    let common = match Common::from(&opts, cmd != "info") {
        Ok(c) => c,
        Err(e) => {
            println!("{}", error_value(&e));
            return 1;
        }
    };

    // 파일이 여럿이면 자식 프로세스로 격리해서 처리한다.
    let value = if common.inputs.len() > 1 {
        supervise(&cmd, &opts, &common)
    } else {
        match cmd.as_str() {
            "info" => run_info_inputs(&common),
            "frame" => run_with_gl(Box::new(move |gl| run_frame(gl, &opts))),
            _ => run_with_gl(Box::new(move |gl| run_sheet(gl, &opts))),
        }
    };
    println!("{}", value);
    if value["ok"] == true { 0 } else { 1 }
}
