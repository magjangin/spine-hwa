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
