# Spine 3.8.99 전용 Viewer Rust 구현 인수인계 문서 (AGENTS.md)

## 1. 프로젝트 개요 & 지원 포맷
- **프로젝트명:** Spine 3.8.99 Viewer (Rust)
- **지원 Spine 데이터 버전:** Spine 3.8.x / 3.8.99 전용 (뮤즈대쉬 등 완벽 지원)
- **실행 바이너리:**
  - `bin/SpineViewer.exe` (더블클릭용 단일 실행파일)
  - `bin/sv-3.8.exe`
- **크레이트 구조 (3개로 압축/단순화 완료):**
  - `crates/spine-sys-38`: Spine 3.8.99 C++ 런타임 컴파일 및 C ABI FFI 바인딩
  - `crates/spine-viewer-core`: Rust 안전 래퍼, OpenGL Glow 렌더러, eframe UI 및 녹화기
  - `crates/sv-38`: 진입점 바이너리
- **구현 언어 및 스택:**
  - Rust 2021 + C++17 (cc-rs)
  - UI & 렌더링: `eframe` (0.37) + OpenGL (`glow`)
  - 좌측 사이드바 도킹 UI (중앙 캐릭터 영역 100% 비가림 구조)
  - 오프스크린 FBO 렌더 텍스처 + `image` crate (초고속 PNG 시퀀스 녹화)

---

## 2. 런타임 백엔드 & 아키텍처 Rationale

### 왜 Spine 3.8.99 단일 타겟 순수 구현인가?
1. **불필요한 버전 분기(37, 40, 41) 완전 제거**:
   - 기존의 다중 버전 매크로(`#if SPINE_VER ...`) 및 37/40/41 관련 크레이트 분할을 완전히 제거하고, C++ 브리지 및 Rust 로직을 오직 Spine 3.8.99 전용으로 간소화하여 코드 줄 수와 유지보수 복잡도를 대폭 줄임.
2. **Spine 3.8.99 완벽 호환 및 안정성**:
   - Spine 바이너리(`.skel`) 및 JSON 포맷 파싱을 `spine-sys-38`을 통해 Spine 3.8 공식 C++ 런타임과 직결.
   - 유니코드 한글 경로(`_wfopen`)를 지원하여 뮤즈대쉬 및 한글 경로의 인게임 에셋을 무결하게 파싱.
3. **단일 독립 실행 파일 (Self-Contained) 배포**:
   - C++ 원본의 SFML 2.6 DLL 의존성을 제거하고, Windows 표준 `opengl32.lib`와 순수 Rust UI(`egui`)를 사용하여 DLL 설치가 필요 없는 단독 바이너리로 완성.
4. **고속 오프스크린 FBO & 비동기 PNG 인코딩**:
   - 최대 바운드(`compute_max_bounds`) 크기의 OpenGL Framebuffer Object(FBO)를 생성하여 투명 배경에 스켈레톤을 렌더링하고, 녹화 시 `glReadPixels`로 실시간 프레임을 읽어 PNG로 즉시 직렬화.

---

## 3. 핵심 인터페이스 및 데이터 파이프라인

- **C ABI 브리지 (`c_src/spine_bridge.h`, `c_src/spine_bridge.cpp`)**:
  - 오직 Spine 3.8.99 공식 C++ 런타임 API만을 사용하여 420여 줄의 간결한 코드로 리팩토링 완료.
  - `spine_create(...)`: 아틀라스 및 바이너리/JSON 데이터 로드 (버전 불일치 시 직관적인 에러 반환)
  - `spine_compute_max_bounds(...)`: 애니메이션 전 구간을 0.016s 단위로 순회하여 실제 생성된 메시 정점 기반 최대 누적 합집합 바운드 계산
  - `spine_generate_mesh(...)`: 드로우 오더 순회, 슬롯/첨부물 유효성 검사, RTTI(Region/Mesh/Clipping) 판별, 클리핑 삼각분할, 블렌드 모드별 배치 분할
- **Rust 안전 래퍼 (`crates/spine-viewer-core/src/spine_wrapper.rs`)**:
  - `spine-sys-38`과 직결되어 RAII 기반 `SpineSkeleton` 수명 관리 (`Drop` 시 자동 메모리 해제)
- **UI 및 조작 (`crates/spine-viewer-core/src/ui.rs`, `app.rs`)**:
  - `좌측 사이드바 (SidePanel)`: 화면 중앙 캐릭터를 절대 가리지 않도록 좌측에 모든 조작 패널 통합
  - `애니메이션 제어`: 재생/일시정지, 정지(0초 리셋), 처음으로, 1프레임 정밀 이동(-1프레임 / +1프레임), 시간 슬라이더(실시간 포즈 연동), 재생 속도(1.0x / 0.5x 버튼), 반복 재생(Loop)
  - `모델 및 뷰 설정`: 외곽 테두리 표시(Border), PMA(알파 미리 곱하기), 위치 X/Y, 확대/축소(100% / 중앙 정렬), 애니메이션/스킨 콤보박스
  - `PNG 시퀀스 녹화`: 녹화 시작/중지, FPS(0.01~60), 해상도 배율(0.01~3.0), 저장 폴더 + `찾기...` 폴더 선택 대화상자, 실시간 진행률 바
  - `현재 프레임 저장`: 시간 슬라이더로 맞춘 시점의 포즈를 PNG 1장으로 저장 (`<애니메이션>_<시간>s.png`, 저장 폴더는 녹화와 공유). 옵션 `내용에 맞춰 자르기`(알파>0 외접 사각형, 기본 ON) / 해제 시 최대 바운드 FBO 크기 그대로. 구현: 매 프레임 렌더되는 FBO를 `read_fbo_pixels`로 읽어 `Recorder::save_snapshot`/`crop_to_content` (`recorder.rs`, 단위 테스트 포함). 해상도 배율은 적용 안 됨(1x). 녹화와 동일하게 픽셀을 그대로 저장하므로 PMA ON 시 가장자리가 어둡게 보일 수 있음(알파 보정 미적용, 인게임 확인상 문제 없음).
  - `애니메이션 시트`: 현재 애니메이션 전 구간을 `FPS` 간격(`ceil(duration*fps)` 프레임)으로 렌더해 격자 PNG 1장(`<애니메이션>_sheet.png`) + 선택적 메타데이터 `.json`(frameWidth/Height, offsetX/Y, scale, columns, rows, frameCount, fps, duration)로 저장. 구현: `App::export_sheet` + `recorder::SheetBuilder`.
    - **셀 크기 Rationale:** 최대 바운드(`compute_max_bounds`)는 이동량이 큰 애니메이션에서 매우 커져(실측 약 824x2160) 시트가 한계를 넘기 때문에, 기본은 2패스 — 1패스로 전 프레임 알파>0 영역의 합집합을 구하고 2패스에서 그 영역으로 잘라 배치(`sheet_crop`). 합집합이라 모든 셀 크기/기준점이 동일해 재생 시 흔들리지 않음.
    - **한계/자동 축소:** 한 변 16384px, 총 1억 픽셀(`SheetBuilder::MAX_SIDE/MAX_PIXELS`) 초과 시 배율을 자동으로 낮춤(Lanczos3 리사이즈). 사용자 배율(`sheet_scale`)과 둘 중 작은 값 적용.
    - 저장 후 원래 시간으로 복원하고 `request_repaint`로 FBO를 원래 포즈로 다시 그림. 해상도 배율(녹화용 `render_scale`)과는 별개.
  - `기본 동작`: 모델 로드 시 자동재생되지 않고 0번 프레임 포즈로 일시정지(`paused: true`) 상태로 대기
  - `한국어 UI`: 특수 이모지 깨짐 없이 순수 한국어 텍스트로 깔끔하게 렌더링
- **모델 열기 (인자 없이 실행 가능)**:
  - 인자 없이 더블클릭 실행 시 검은 콘솔창 없이 한국어 환영 대시보드가 뜨며, `스켈레톤 파일 열기...` 버튼 또는 드래그 앤 드롭으로 연다. 인자 1개(exe 위에 파일 드롭)나 2~3개(기존 CLI)도 그대로 동작.
  - `.atlas`와 `.skel`/`.json` 중 한쪽만 선택해도 같은 폴더의 같은 이름 파일을 찾는다 (`cli::resolve_paths`).

---

## 4. 빌드 명령 및 산출물 배포 경로

### 전체 릴리즈 빌드 & 배포
```powershell
# dist.ps1 실행 시 릴리즈 빌드 후 bin/ 폴더로 복사 배포
powershell -ExecutionPolicy Bypass -File .\dist.ps1
```

### 개별 빌드 명령
```powershell
cargo build --release -p sv-38
```

### 산출물 위치
- `bin/SpineViewer.exe` (Spine 3.8.99 Viewer 단독 실행 파일)
- `bin/sv-3.8.exe`
- `bin/sv-cli.exe` (헤드리스 CLI, 콘솔 서브시스템 — MCP/자동화용, 아래 5장)

---

## 5. 헤드리스 CLI(`sv-cli`) & MCP 서버 (AI 자동화)

뮤즈대쉬 에셋(`H:\유튜브 곡 모음\음악원본 저작권 O\뮤즈대쉬\gamescene_추출\장면NN\원본`)의 몬스터/보스를 Spine Pro 없이 일괄 렌더하려고 만든 경로. asset-tool(`H:\source\repos\asset tool`)이 추출한 `.skel/.atlas/.png`를 그대로 읽는다 (파츠 분리·Spine Pro 임포트 스케일 불필요).

- **구성:** `crates/spine-viewer-core/src/headless.rs`(CLI 본체) · `export.rs`(GUI와 공용 렌더/시트 로직) · `crates/sv-38/src/cli_main.rs`(`sv-cli` 진입점) · `mcp/spine_mcp.py`(표준 라이브러리만 쓰는 stdio MCP 서버, `sv-cli.exe`를 호출) · `.mcp.json`(Claude Code 등록).
- **명령:** `sv-cli info|frame|frames|sheet <스켈레톤|폴더> ...` (옵션은 `headless.rs` 상단 주석). 결과는 stdout에 JSON 한 덩어리(`ok/count/results/skipped`).
- **MCP 도구:** `spine_info`, `spine_render_frame`, `spine_render_sheet`, `spine_contact_sheet`, `spine_render_recipe`. 등록은 이 폴더의 `.mcp.json`(Claude Code 재시작 필요). 다른 프로젝트에서도 쓰려면 `claude mcp add spine-viewer -- python "H:\source\repos\spine hwa\mcp\spine_mcp.py"`.
- **AI 작업 흐름(어긋나는 프레임 고르기):** ① `spine_render_frame`(폴더, skin=all, scale=0.5)로 전부 뽑는다 → ② 이상한 것(비어 있음/찌그러짐/standby 없음)만 `spine_contact_sheet`로 애니메이션×시점 시트(행=애니메이션, 열=시점, 라벨 포함)를 보고 프레임을 고른다 → ③ `spine_render_recipe`(entries: path/animation/time/skin/name/pair)로 일괄 재렌더. `pair=true`는 `_air`↔`_road` 짝을 같은 애니메이션·시점으로 같이 뽑는다. 컨택트 시트는 `sv-cli frames`(애니메이션별 N개 시점을 크롭 PNG로 저장)를 MCP(Python, **Pillow 필요**)가 라벨 붙여 합성한다. 시험: 0107(`in_boss_2_17`@0.55 → 100x70, 기존 104x71), 0108(`standby`@0.44 → 111x85, 기존 115x89), 롱노트 0115(`in_16_old`@0.85 → 573x288, 기존 579x309)가 시트에서 고른 시점으로 기존과 같은 종류의 프레임이 나왔다.
- **뮤즈대쉬 기준값:** 스켈레톤 원본 크기가 Spine Pro 결과(임포트 스케일 0.5)의 2배다 → 기존 `몬스터_렌더`와 맞추려면 `--scale 0.5`. 기존 결과와 비교하면 standby 0초 프레임은 대부분 ±3px로 일치하고(장면01~12 전체 비교), 어긋나는 건 사용자가 손으로 고른 프레임(`pick: largest` 등)과 standby 없는 스켈레톤이다.
  - **장면01 MCP 검증(2026-10-19, `spine_render_frame` skin=all, scale=0.5, 52개 전부 렌더·skipped 0):** 기존 `몬스터_렌더/장면01` 24장 중 16장이 ±4px 일치(그림도 육안 동일, 그림자 유지/제거 판단 포함). 0106 air/road는 `time=largest`로 일치(+2). 남은 6장은 0107(standby 없음, `in_boss_2_17`의 손으로 고른 자세), 0108(기존은 다른 프레임), 0115(standby 없는 롱노트 실패)라서 자동 선택으론 안 맞는다 → 미리보기 보고 `time`을 지정한다.
  - **전 장면 검증(장면01~12, 기존 `몬스터_렌더` 298장, skin=all scale=0.5, 렌더 전부 성공·skipped 0, 장면당 약 13~20초):** 기본(standby 0초 / 비면 largest 대체) 191장(64%)이 ±4px 일치, `--time largest`는 전체 적용 시 96장뿐(사용자도 일부에만 largest를 썼다), 둘 중 하나라도 맞는 건 202장. 안 맞는 96장은 대부분 사용자가 손으로 고른 자세다: standby 없는 `in_*` 연출(0107/0206/0307/0506/0706/1208 등), 롱노트 `xx15`(`in_16_old`/`in_18`, 전 장면), `xx16`(`in_up_24`), 그리고 standby 첫 프레임이 찌그러져 있어 사용자가 다른 프레임을 고른 `xx06~08` 일부. 같은 이름 자동 선택으로는 재현 불가 → 미리보기를 보고 `time`을 지정하는 흐름으로 쓴다.
  - **크롭 알파 기준(`EDGE_ALPHA=1`, `HALO_ROWS=16`):** 처음엔 덩어리 구분용 임계(알파>8)로 위아래 경계까지 잡았더니 장면07/08에서 기존보다 5~8px(0.5배 기준) 작게 나왔다. 글로우 같은 옅은 번짐이 Spine Pro 크롭에는 포함되기 때문(0708_air: 알파>0=333행, >1=327, >2=323, 기존=약324). 그래서 본체 덩어리 위아래로 옅은 줄(알파>1)을 최대 16줄까지 포함한다. 장면07 일치 6→20장. (알파 임계를 2로 하면 가로가 1~5px 모자라는 경우가 있어 1로 정함. Spine Pro 크롭이 순수 임계 방식이 아니라 완전 일치는 불가.)

### 설계 Rationale / 함정
1. **숨김 창으로 GL 컨텍스트 확보:** eframe `with_visible(false)` 창의 첫 `ui()` 프레임에서 작업을 실행하고 `Close`한다 (별도 EGL/오프스크린 컨텍스트 불필요, 실측 동작). 렌더러는 GUI와 같은 `GlRenderer`/FBO 경로를 그대로 쓴다.
2. **폴더 입력은 스켈레톤마다 자기 자신을 자식 프로세스로 실행:** Spine C++ 런타임은 엉뚱한 아틀라스로 로드하면 예외가 아니라 `assert(inIndex < _size)`로 프로세스를 죽인다(예: 0304.skel + 0301.atlas). 격리해서 해당 파일만 `skipped`로 남기고 계속 진행한다. (파일당 약 0.3초)
3. **아틀라스 자동 짝맞춤(스켈레톤 ↔ 아틀라스는 1:1이 아님):** 같은 폴더 `.atlas`의 리전 이름이 스켈레톤 바이너리 안 문자열과 얼마나 겹치는지로 점수 → 높은 순, 동점이면 ①같은 이름 ②다른 스켈레톤 전용 이름이 아닌 것(예: `0107_air_double.atlas`는 `0107_air_double.skel` 전용이라 뒤로) ③접두사 순. 겹침 0인 후보는 시도조차 안 한다(어서션 방지). 틀리면 `--atlas`로 직접 지정.
4. **알파 미리 곱하기 복원:** PMA로 그린 FBO를 그대로 PNG로 저장하면 반투명 효과가 어둡게 나온다(0105 빛줄기로 확인). `render_pose`와 GUI "현재 프레임 저장"은 PMA일 때 `Recorder::unpremultiply`를 적용한다. (GUI 녹화 기능은 기존 그대로 미적용)
5. **그림자 크롭 규칙(실측으로 맞춤):** 본체(납작하지 않은 가장 큰 덩어리)에서 세로로 `max(20px, 본체 높이의 15%)` 이상 떨어진 납작한(높이 ≤ 폭의 절반) 덩어리는 크롭에서 뺀다 (`--keep-shadow`로 끔). 가까운 그림자(0111: 간격 22px)는 기존 결과처럼 남기고 먼 것(0110: 38px, 0105_road: 70px)은 뺀다.
6. **`--time`:** 숫자(초) 또는 `largest`(본체 면적이 가장 큰 프레임, 1/30초 간격 최대 150샘플). 시간을 지정하지 않았는데 그 시점이 비어 있으면(in_/out_ 연출 첫 프레임 등) 자동으로 largest로 대신한다. 기본 애니메이션은 `standby`, 없으면 첫 번째.
7. **스킨:** `--skin all`은 default 외 모든 스킨을 `<이름>__<스킨>`으로 각각 저장한다 (몸통이 스킨에만 있는 0301/0307/0314/0914 등). 스킨 없이 뽑으면 비어 보인다. 런타임의 `setSkin`은 default 스킨을 폴백으로 쓰므로 Spine Pro처럼 JSON 복사본을 만들 필요가 없다.

### Dead Ends / 아직 못한 것
- `_air`/`_road` 쌍을 같은 애니메이션·같은 시점으로 묶어 고르는 기능은 없다 (결과의 `time`을 보고 두 번째 호출에 `--time`으로 지정).
- 사용자가 손으로 고른 프레임(예: 0316)과 자동 `largest`는 일치하지 않을 수 있다 → 미리보기를 보고 시간을 지정해 다시 뽑는 식으로 쓴다.
- 보스 `animations.json`/컨택트 시트 같은 asset-tool 쪽 출력 형식은 만들지 않았다 (시트는 `spine_render_sheet`의 json 형식).
