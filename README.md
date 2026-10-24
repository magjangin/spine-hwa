# Spine 3.8.99 Viewer & Headless CLI (`spine-hwa`)

> **Rust + Spine 3.8.99 공식 C++ 런타임 기반의 고속 스파인 뷰어, 배치 렌더러 CLI 및 AI MCP 서버**

[![Platform](https://img.shields.io/badge/Platform-Windows_x64-blue.svg)](https://github.com/magjangin/spine-hwa)
[![Spine Version](https://img.shields.io/badge/Spine-3.8.x_%2F_3.8.99-orange.svg)](http://esotericsoftware.com/)
[![Rust](https://img.shields.io/badge/Rust-2021_Edition-red.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

---

## 📌 개요

`spine-hwa`는 **Spine 3.8.x / 3.8.99** 버전으로 제작된 2D 스켈레톤 애니메이션 에셋(뮤즈대쉬 등 게임 에셋)을 가볍고 정확하게 미리보고, 고품질 투명 PNG 시퀀스나 스프라이트 시트로 추출하기 위해 개발된 도구입니다.

기존 C++ 원본의 SFML 2.6 DLL 의존성을 완전히 제거하고, **Rust (`eframe` + `glow` OpenGL)** 환경으로 재설계하여 **단일 실행 바이너리(Self-Contained)**로 동작합니다. 또한 대량의 에셋을 자동 렌더링할 수 있는 **헤드리스 CLI (`sv-cli`)**와 Claude Code / AI 에이전트가 직접 제어할 수 있는 **MCP (Model Context Protocol) 서버**를 기본 제공합니다.

---

## ✨ 주요 기능

### 1. Spine 3.8.99 전용 완벽 호환
- Spine 공식 C++ 3.8.99 런타임을 C ABI 브리지로 직결하여 바이너리(`.skel`) 및 `.json` 포맷을 100% 무결하게 파싱합니다.
- 복잡한 클리핑(Clipping), 메시 변형(Deform), 블렌드 모드(Normal, Add, Multiply, Screen), PMA(Premultiplied Alpha)를 완벽하게 재현합니다.
- **완벽한 유니코드 및 한글 경로 지원 (`_wfopen`)**: 경로에 한글이나 공백이 포함되어 있어도 오류 없이 로드됩니다.

### 2. 가볍고 깔끔한 GUI 뷰어 (`SpineViewer.exe`)
- **중앙 캔버스 100% 비가림 구조**: 좌측 사이드바 도킹 UI를 채택하여 캐릭터가 UI에 가려지지 않습니다.
- **애니메이션 제어**: 재생/일시정지, 처음으로, 1프레임 정밀 이동(-1 / +1 프레임), 타임라인 슬라이더, 속도 조절(1.0x / 0.5x), 반복 재생(Loop).
- **편의 기능**: 스켈레톤 외곽 테두리(Border) 토글, 스킨/애니메이션 콤보박스, 줌/이동, 캔버스 중앙 정렬.
- **투명 배경 스냅샷**: 현재 타임라인의 포즈를 알파 투명 PNG 1장으로 즉시 저장 (내용물에 맞춘 자동 크롭 지원).
- **고속 PNG 시퀀스 녹화**: FBO 픽셀 고속 읽기(`glReadPixels`) 기반 실시간 PNG 시퀀스 추출.
- **원클릭 스프라이트 시트(애니메이션 시트) 생성**:
  - 전 프레임 알파 영역의 합집합을 2패스로 계산하여 흔들림 없이 딱 맞는 셀 크기로 격자 PNG 시트(`<name>_sheet.png`)를 렌더링합니다.
  - 프레임 크기, 오프셋, 스케일, 행/열 정보가 담긴 `.json` 메타데이터를 함께 출력합니다.
- **단독 실행**: 인자 없이 더블클릭하면 친절한 한국어 시작 화면이 표시되며, 파일 탐색기에서 스켈레톤 파일을 드래그 앤 드롭하여 바로 열 수 있습니다.

### 3. 헤드리스 배치 CLI (`sv-cli.exe`)
- Spine Pro 에디터가 없어도 폴더 내 수백 개의 스켈레톤 파일을 콘솔 명령 한 줄로 자동 일괄 렌더링합니다.
- **스마트 아틀라스 짝맞춤 (Heuristic Atlas Matching)**: 스켈레톤 내부 리전명과 동일 폴더 내 아틀라스의 텍스처 리전명을 교차 분석하여 가장 적합한 `.atlas`를 자동으로 찾아 연결합니다.
- 프로세스 격리 실행 구조로 불량 데이터가 있어도 전체 배치가 중단되지 않고 건너뜁니다.

### 4. AI 에이전트 연동용 MCP 서버 내장
- Anthropic Claude Code 및 Antigravity 등 차세대 코딩 어시스턴트와 연결 가능한 MCP 도구 제공 (`mcp/spine_mcp.py`):
  - `spine_info`: 스켈레톤 내 애니메이션, 스킨 목록 및 바운드 정보 조회
  - `spine_render_frame`: 지정된 시간/애니메이션의 고화질 프레임 크롭 렌더링
  - `spine_render_sheet`: 스프라이트 시트 및 JSON 메타데이터 일괄 추출
  - `spine_contact_sheet`: 애니메이션×시간대별 타임테이블 컨택트 시트 자동 생성
  - `spine_render_recipe`: 레시피 JSON 기반 조건부 정밀 일괄 렌더링

---

## 🏗️ 프로젝트 구조

```
spine-hwa/
├── bin/                       # 빌드된 단독 실행 바이너리
│   ├── SpineViewer.exe        # GUI 스파인 뷰어 (단독 실행)
│   ├── sv-3.8.exe             # GUI 스파인 뷰어 (콘솔/배포 호환)
│   └── sv-cli.exe             # 헤드리스 배치 렌더러 CLI
├── c_src/                     # Spine 공식 3.8.99 C++ 런타임 및 C ABI 브리지
│   ├── spine/                 # 공식 런타임 헤더
│   ├── spine_bridge.h
│   └── spine_bridge.cpp
├── crates/
│   ├── spine-sys-38/          # Spine C++ 런타임 컴파일 (cc-rs) 및 Rust FFI 바인딩
│   ├── spine-viewer-core/     # 렌더러(glow), GUI(eframe), 레코더, 헤드리스 렌더 엔진
│   └── sv-38/                 # 진입점 바이너리 (GUI & CLI)
├── mcp/
│   └── spine_mcp.py           # 표준 라이브러리 기반 MCP stdio 서버
├── dist.ps1                   # 원클릭 릴리즈 빌드 & 배포 스크립트
├── Cargo.toml
└── AGENTS.md                  # 상세 아키텍처 및 개발 인수인계 문서
```

---

## 🚀 시작하기

### 사전 요구사항
- **OS**: Windows 10/11 x64
- **Rust Toolchain**: 1.75 이상 (`rustup default stable-msvc`)
- **C++ 빌드 도구**: Visual Studio 2019/2022 C++ Build Tools (MSVC)
- **Python**: 3.10+ (MCP 서버 구동 시, 컨택트 시트 합성을 위해 `pip install Pillow` 권장)

### 빌드 및 배포

```powershell
# 1. 저장소 클론
git clone https://github.com/magjangin/spine-hwa.git
cd spine-hwa

# 2. 전체 릴리즈 빌드 및 bin/ 배포 스크립트 실행
powershell -ExecutionPolicy Bypass -File .\dist.ps1
```

빌드가 완료되면 `bin/` 폴더에 `SpineViewer.exe`, `sv-3.8.exe`, `sv-cli.exe`가 생성됩니다.

---

## 🎮 사용 방법

### 1. GUI 뷰어 실행
- `bin/SpineViewer.exe`를 더블클릭하여 실행합니다.
- 시작 화면의 **[스켈레톤 파일 열기...]** 버튼을 누르거나, `.skel` / `.json` 파일을 창 위로 드래그 앤 드롭합니다.
- 같은 폴더에 같은 이름의 `.atlas` 파일이 있으면 자동으로 함께 로드됩니다.

### 2. 헤드리스 CLI (`sv-cli`) 사용
터미널에서 직접 명령어를 실행하여 정보를 확인하거나 이미지를 추출합니다.

```powershell
# 스켈레톤 내부 정보(애니메이션, 스킨, 크기) JSON 출력
.\bin\sv-cli.exe info path\to\character.skel

# 특정 애니메이션의 특정 시간(초) 포즈 렌더링
.\bin\sv-cli.exe frame path\to\character.skel -a idle -t 0.5 -o output.png

# 가장 면적이 큰 프레임을 자동 감지하여 0.5배율로 렌더링 (그림자 자동 제거)
.\bin\sv-cli.exe frame path\to\character.skel -a attack -t largest --scale 0.5 -o output.png

# 폴더 내 모든 스켈레톤 일괄 렌더링 (모든 스킨 포함)
.\bin\sv-cli.exe frame path\to\monsters_dir --skin all --scale 0.5 -o rendered_frames

# 스프라이트 시트 생성 (30 FPS)
.\bin\sv-cli.exe sheet path\to\character.skel -a run --fps 30 -o run_sheet.png
```

### 3. Claude Code / AI MCP 연동
Claude Code 프로젝트 설정(`.mcp.json`)에 아래와 같이 등록하여 AI가 스파인 에셋을 직접 시각화하고 대량 추출하도록 지원합니다:

```json
{
  "mcpServers": {
    "spine-viewer": {
      "command": "python",
      "args": ["H:/source/repos/spine hwa/mcp/spine_mcp.py"]
    }
  }
}
```

---

## 📄 라이선스

- 본 프로젝트의 자체 소스 코드는 [MIT License](LICENSE)를 따릅니다.
- 포함된 **Spine Runtimes**는 Esoteric Software의 Spine Runtimes License에 따라 Spine 라이선스 보유자에게 사용 권한이 부여됩니다. 자세한 내용은 `c_src/` 디렉터리의 라이선스 고지를 참조하십시오.
