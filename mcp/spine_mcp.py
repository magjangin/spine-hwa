#!/usr/bin/env python3
"""Spine 3.8.99 뷰어(sv-cli.exe)를 AI가 호출하는 MCP 서버 (stdio, 표준 라이브러리만 사용).

도구:
  spine_info          스켈레톤의 아틀라스 짝/스킨/애니메이션(길이) 조회
  spine_render_frame  지정한 시점의 통짜 PNG 추출 (폴더를 주면 일괄)
  spine_render_sheet  애니메이션 전 구간 스프라이트 시트 PNG + json 추출

실행 파일 위치: 환경변수 SV_CLI, 없으면 <이 파일>/../bin/sv-cli.exe
"""
import base64
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SV_CLI = os.environ.get("SV_CLI") or os.path.join(HERE, "..", "bin", "sv-cli.exe")
MAX_PREVIEW_IMAGES = 4
MAX_PREVIEW_BYTES = 1_500_000

COMMON_PATH = {
    "type": "string",
    "description": "스켈레톤(.skel/.json) 파일 또는 폴더. 폴더면 하위 전부를 처리하고, 아틀라스는 같은 폴더에서 "
                   "스켈레톤이 쓰는 리전 이름이 가장 많이 겹치는 .atlas 를 자동으로 고른다.",
}
ATLAS = {"type": "string", "description": "아틀라스(.atlas) 직접 지정 (자동 짝맞춤이 틀릴 때만)."}
ANIMATION = {"type": "string", "description": "애니메이션 이름. 생략하면 'standby', 없으면 첫 번째."}
SKIN = {
    "type": "string",
    "description": "스킨 이름, 또는 'all'(default 외 모든 스킨을 각각 `<이름>__<스킨>` 으로). 생략하면 기본 스킨. "
                   "몸통 등이 default 가 아닌 스킨에만 있는 스켈레톤(뮤즈대쉬 0301, 0307, 0314, 0914 등)은 "
                   "스킨 없이 뽑으면 비어 보이므로 spine_info 로 스킨을 확인하고 'all' 을 쓴다.",
}

TOOLS = [
    {
        "name": "spine_info",
        "description": "스켈레톤의 짝 아틀라스, 스킨 목록, 애니메이션 이름/길이(초)를 조회한다. 렌더 전에 어떤 애니메이션/스킨이 있는지 볼 때 쓴다.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": COMMON_PATH, "atlas": ATLAS},
            "required": ["path"],
        },
    },
    {
        "name": "spine_render_frame",
        "description": (
            "애니메이션의 한 시점을 투명 배경 통짜 PNG로 렌더한다. 폴더를 주면 모든 스켈레톤을 일괄 렌더한다. "
            "스켈레톤 원본 크기 기준이며, 뮤즈대쉬 몬스터/보스를 Spine Pro 임포트 스케일 0.5로 뽑던 결과와 같게 하려면 scale=0.5 를 쓴다. "
            "본체에서 멀리 떨어진 납작한 바닥 그림자는 기본으로 크롭에서 뺀다. "
            "결과에는 쓰인 시간(time)과 파일 경로가 담기고, 작은 결과는 미리보기 이미지로 함께 돌려준다."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": COMMON_PATH,
                "out": {"type": "string", "description": "출력 폴더 (단일 스켈레톤+단일 스킨이면 .png 파일 경로도 가능). 폴더 입력이면 하위 구조를 따라간다."},
                "animation": ANIMATION,
                "time": {"type": "string", "description": "초 단위 숫자(기본 0), 또는 'largest'(애니메이션에서 본체가 가장 큰 프레임). standby 첫 프레임이 찌그러져 있거나 비어 있을 때 largest 를 쓴다."},
                "skin": SKIN,
                "scale": {"type": "number", "description": "출력 배율 0.01~1.0 (기본 1.0). 뮤즈대쉬는 0.5 권장."},
                "crop": {"type": "boolean", "description": "내용에 맞춰 자르기 (기본 true). false면 애니메이션 최대 바운드 크기."},
                "keep_shadow": {"type": "boolean", "description": "true면 멀리 떨어진 납작한 그림자도 크롭에 포함 (기본 false)."},
                "pma": {"type": "boolean", "description": "알파 미리 곱하기 렌더 (기본 true, 뮤즈대쉬 에셋은 true)."},
                "atlas": ATLAS,
                "preview": {"type": "boolean", "description": "결과 이미지를 응답에 첨부 (기본 true, 최대 4장)."},
            },
            "required": ["path", "out"],
        },
    },
    {
        "name": "spine_render_sheet",
        "description": (
            "애니메이션 전 구간을 fps 간격으로 렌더해 격자 스프라이트 시트 PNG 한 장과 메타데이터 json"
            "(frameWidth/Height, offsetX/Y, columns, rows, frameCount, fps, duration, scale)을 만든다. "
            "셀 크기는 전 프레임 내용의 합집합이라 모든 칸이 같은 크기/기준점이다. 한계(한 변 16384px, 총 1억 픽셀)를 넘으면 자동 축소한다."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": COMMON_PATH,
                "out": {"type": "string", "description": "출력 폴더."},
                "animation": ANIMATION,
                "skin": SKIN,
                "fps": {"type": "number", "description": "초당 프레임 (기본 24)."},
                "cols": {"type": "integer", "description": "시트 열 수 (기본 8)."},
                "scale": {"type": "number", "description": "시트 배율 0.01~1.0 (기본 1.0). 뮤즈대쉬는 0.5 권장."},
                "crop": {"type": "boolean", "description": "셀을 전 프레임 내용 합집합으로 줄이기 (기본 true)."},
                "keep_shadow": {"type": "boolean", "description": "true면 납작한 바닥 그림자도 셀 영역에 포함 (기본 false)."},
                "write_json": {"type": "boolean", "description": "메타데이터 json 저장 (기본 true)."},
                "pma": {"type": "boolean", "description": "알파 미리 곱하기 렌더 (기본 true)."},
                "atlas": ATLAS,
            },
            "required": ["path", "out"],
        },
    },
]


def build_args(name, a):
    cmd = {"spine_info": "info", "spine_render_frame": "frame", "spine_render_sheet": "sheet"}[name]
    args = [cmd, a["path"]]

    def opt(flag, key, conv=str):
        if a.get(key) is not None:
            args.extend([flag, conv(a[key])])

    opt("--out", "out")
    opt("--atlas", "atlas")
    opt("--animation", "animation")
    opt("--skin", "skin")
    opt("--scale", "scale")
    opt("--time", "time")
    opt("--fps", "fps")
    opt("--cols", "cols")
    if a.get("crop") is False:
        args.append("--no-crop" if name == "spine_render_frame" else "--no-sheet-crop")
    if a.get("keep_shadow"):
        args.append("--keep-shadow")
    if a.get("pma") is False:
        args.append("--no-pma")
    if a.get("write_json") is False:
        args.append("--no-json")
    return args


def run_cli(args):
    if not os.path.isfile(SV_CLI):
        raise RuntimeError("sv-cli.exe 를 찾을 수 없습니다: %s (dist.ps1 로 빌드하거나 SV_CLI 환경변수를 지정하세요)" % SV_CLI)
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    proc = subprocess.run([SV_CLI] + args, capture_output=True, creationflags=flags, timeout=3600)
    out = proc.stdout.decode("utf-8", "replace")
    for line in reversed(out.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                break
    err = proc.stderr.decode("utf-8", "replace").strip()
    raise RuntimeError("sv-cli 결과를 읽지 못했습니다 (종료 코드 %s): %s" % (proc.returncode, err or out.strip() or "출력 없음"))


def call_tool(name, a):
    result = run_cli(build_args(name, a))
    content = [{"type": "text", "text": json.dumps(result, ensure_ascii=False, indent=1)}]
    if name == "spine_render_frame" and a.get("preview", True) and result.get("ok"):
        sent = 0
        for r in result.get("results", []):
            if sent >= MAX_PREVIEW_IMAGES:
                break
            p = r.get("output")
            if p and os.path.isfile(p) and os.path.getsize(p) <= MAX_PREVIEW_BYTES:
                with open(p, "rb") as f:
                    content.append({"type": "image", "data": base64.b64encode(f.read()).decode("ascii"), "mimeType": "image/png"})
                sent += 1
    return {"content": content, "isError": not result.get("ok", False)}


def respond(msg_id, result=None, error=None):
    msg = {"jsonrpc": "2.0", "id": msg_id}
    if error is not None:
        msg["error"] = error
    else:
        msg["result"] = result
    sys.stdout.write(json.dumps(msg, ensure_ascii=True) + "\n")
    sys.stdout.flush()


def handle(msg):
    method = msg.get("method")
    msg_id = msg.get("id")
    if method == "initialize":
        version = (msg.get("params") or {}).get("protocolVersion", "2024-11-05")
        respond(msg_id, {
            "protocolVersion": version,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "spine-viewer", "version": "0.1.0"},
        })
    elif method == "ping":
        respond(msg_id, {})
    elif method == "tools/list":
        respond(msg_id, {"tools": TOOLS})
    elif method == "tools/call":
        params = msg.get("params") or {}
        try:
            respond(msg_id, call_tool(params.get("name"), params.get("arguments") or {}))
        except Exception as e:  # 도구 오류는 isError 결과로 돌려준다
            respond(msg_id, {"content": [{"type": "text", "text": "오류: %s" % e}], "isError": True})
    elif msg_id is not None:
        respond(msg_id, error={"code": -32601, "message": "지원하지 않는 메서드: %s" % method})


def main():
    stdin = sys.stdin.buffer
    for raw in stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            handle(json.loads(raw.decode("utf-8")))
        except Exception as e:
            sys.stderr.write("spine_mcp 처리 오류: %s\n" % e)
            sys.stderr.flush()


if __name__ == "__main__":
    main()
