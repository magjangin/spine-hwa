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
    {
        "name": "spine_contact_sheet",
        "description": (
            "스켈레톤의 애니메이션별 후보 프레임을 라벨 붙은 컨택트 시트 한 장으로 만든다 (행=애니메이션, 열=시점). "
            "어느 애니메이션의 몇 초가 통짜로 제일 잘 보이는지 눈으로 고를 때 쓴다 (standby 첫 프레임이 찌그러졌거나 standby 가 없을 때). "
            "고른 값은 spine_render_recipe 로 일괄 렌더한다. 폴더를 주면 스켈레톤마다 시트를 하나씩 만든다. "
            "Pillow(PIL)가 필요하다."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": COMMON_PATH,
                "out": {"type": "string", "description": "출력 폴더. 시트는 `<이름>_contact.png`, 개별 프레임은 `_frames/` 아래."},
                "animations": {"type": "string", "description": "쉼표로 구분한 애니메이션 이름들 (생략하면 전부). 많으면 'standby,in_nor_33' 처럼 줄인다."},
                "samples": {"type": "integer", "description": "애니메이션당 시점 수 (기본 5, 최대 24). 시점은 0, 길이/N, 2*길이/N ... (끝 제외)."},
                "skin": {"type": "string", "description": "스킨 이름 또는 'all' (생략하면 기본 스킨)."},
                "scale": {"type": "number", "description": "프레임 렌더 배율 (기본 0.5)."},
                "cell": {"type": "integer", "description": "시트 칸 크기 px (기본 140)."},
                "keep_shadow": {"type": "boolean", "description": "true면 먼 그림자도 크롭에 포함."},
                "atlas": ATLAS,
                "preview": {"type": "boolean", "description": "시트 이미지를 응답에 첨부 (기본 true, 최대 4장)."},
            },
            "required": ["path", "out"],
        },
    },
    {
        "name": "spine_render_recipe",
        "description": (
            "레시피(스켈레톤마다 애니메이션/시점)대로 통짜 PNG를 일괄 렌더한다. 컨택트 시트에서 고른 값을 그대로 넣는다. "
            "entries 의 각 항목: {path, animation?, time?, skin?, name?, pair?}. time 은 초 숫자 또는 'largest'. "
            "pair=true 면 같은 폴더의 `_air`↔`_road` 짝도 같은 애니메이션/시점으로 같이 렌더한다. "
            "출력 파일명은 `<name 또는 스켈레톤 이름>[__스킨].png`."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "entries": {
                    "type": "array",
                    "description": "렌더할 항목 목록. recipe_file 을 쓰면 생략.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "스켈레톤 파일 경로 (.skel/.json)."},
                            "animation": {"type": "string"},
                            "time": {"type": ["number", "string"], "description": "초 또는 'largest'."},
                            "skin": {"type": "string", "description": "스킨 이름 또는 'all'."},
                            "name": {"type": "string", "description": "출력 파일 이름(확장자 제외). 생략하면 스켈레톤 이름."},
                            "pair": {"type": "boolean", "description": "_air/_road 짝도 같이 렌더."},
                        },
                        "required": ["path"],
                    },
                },
                "recipe_file": {"type": "string", "description": "entries 배열(또는 {\"entries\": [...]}) 이 담긴 JSON 파일 경로."},
                "out": {"type": "string", "description": "출력 폴더."},
                "scale": {"type": "number", "description": "출력 배율 (기본 1.0, 뮤즈대쉬는 0.5)."},
                "keep_shadow": {"type": "boolean"},
                "crop": {"type": "boolean"},
                "pma": {"type": "boolean"},
                "workers": {"type": "integer", "description": "동시 처리 수 (기본 4)."},
                "preview": {"type": "boolean", "description": "결과 이미지를 응답에 첨부 (기본 false, 최대 4장)."},
            },
            "required": ["out"],
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


def attach_images(content, paths):
    sent = 0
    for p in paths:
        if sent >= MAX_PREVIEW_IMAGES:
            break
        if p and os.path.isfile(p) and os.path.getsize(p) <= MAX_PREVIEW_BYTES:
            with open(p, "rb") as f:
                content.append({"type": "image", "data": base64.b64encode(f.read()).decode("ascii"), "mimeType": "image/png"})
            sent += 1


def _font(size):
    from PIL import ImageFont
    for p in (r"C:\Windows\Fonts\consola.ttf", r"C:\Windows\Fonts\malgun.ttf"):
        try:
            return ImageFont.truetype(p, size)
        except Exception:
            pass
    return ImageFont.load_default()


def make_contact_pages(entry, out_dir, cell):
    """frames 결과 한 항목(스켈레톤 하나)을 라벨 붙은 컨택트 시트 PNG(들)로 만든다."""
    from PIL import Image, ImageDraw

    anims = entry["animations"]
    name = os.path.basename(entry["dir"].rstrip("\\/"))
    label_w, cap_h, pad = 170, 16, 6
    cols = max((len(a["frames"]) for a in anims), default=1)
    rows_per_page = 10
    font, small = _font(13), _font(11)
    pages = []
    for pi in range(0, len(anims), rows_per_page):
        chunk = anims[pi:pi + rows_per_page]
        w = label_w + cols * (cell + pad) + pad
        h = len(chunk) * (cell + cap_h + pad) + pad
        sheet = Image.new("RGB", (w, h), (36, 39, 46))
        d = ImageDraw.Draw(sheet)
        for r, a in enumerate(chunk):
            y0 = pad + r * (cell + cap_h + pad)
            d.text((pad, y0 + 2), a["name"], fill=(235, 235, 120), font=font)
            d.text((pad, y0 + 20), "len %.2fs" % a["duration"], fill=(150, 155, 165), font=small)
            d.text((pad, y0 + 36), "row %d" % (pi + r), fill=(110, 115, 125), font=small)
            for c, fr in enumerate(a["frames"]):
                x0 = label_w + c * (cell + pad)
                d.rectangle([x0, y0 + cap_h, x0 + cell - 1, y0 + cap_h + cell - 1], fill=(52, 56, 64), outline=(80, 84, 92))
                d.text((x0 + 2, y0), "#%d  t=%.2f" % (fr["index"], fr["time"]), fill=(190, 200, 215), font=small)
                if fr.get("file") and os.path.isfile(fr["file"]):
                    im = Image.open(fr["file"]).convert("RGBA")
                    s = min((cell - 4) / im.width, (cell - 4) / im.height, 4.0)
                    im = im.resize((max(1, int(im.width * s)), max(1, int(im.height * s))), Image.LANCZOS)
                    sheet.paste(im, (x0 + (cell - im.width) // 2, y0 + cap_h + (cell - im.height) // 2), im)
                else:
                    d.text((x0 + 6, y0 + cap_h + cell // 2 - 6), "(empty)", fill=(120, 125, 135), font=small)
        suffix = "" if len(anims) <= rows_per_page else "_%d" % (pi // rows_per_page + 1)
        path = os.path.join(out_dir, "%s_contact%s.png" % (name, suffix))
        sheet.save(path)
        pages.append(path)
    return pages


def tool_contact_sheet(a):
    try:
        import PIL  # noqa: F401
    except ImportError:
        raise RuntimeError("컨택트 시트에는 Pillow 가 필요합니다: python -m pip install pillow")
    out = a["out"]
    os.makedirs(out, exist_ok=True)
    args = ["frames", a["path"], "--out", os.path.join(out, "_frames")]
    anims = a.get("animations")
    if isinstance(anims, list):
        anims = ",".join(anims)
    for flag, val in (("--animations", anims), ("--samples", a.get("samples")), ("--skin", a.get("skin")),
                      ("--scale", a.get("scale", 0.5)), ("--atlas", a.get("atlas"))):
        if val is not None:
            args.extend([flag, str(val)])
    if a.get("keep_shadow"):
        args.append("--keep-shadow")
    result = run_cli(args)
    if not result.get("ok"):
        return {"content": [{"type": "text", "text": json.dumps(result, ensure_ascii=False, indent=1)}], "isError": True}
    cell = int(a.get("cell", 140))
    sheets, images = [], []
    for entry in result["results"]:
        pages = make_contact_pages(entry, out, cell)
        images.extend(pages)
        sheets.append({
            "input": entry["input"],
            "atlas": entry["atlas"],
            "skin": entry["skin"],
            "images": pages,
            "frames_dir": entry["dir"],
            "rows": [{"row": i, "animation": an["name"], "duration": an["duration"],
                      "times": [f["time"] for f in an["frames"]]} for i, an in enumerate(entry["animations"])],
        })
    summary = {"ok": True, "count": len(sheets), "sheets": sheets, "skipped": result.get("skipped", [])}
    content = [{"type": "text", "text": json.dumps(summary, ensure_ascii=False, indent=1)}]
    if a.get("preview", True):
        attach_images(content, images)
    return {"content": content, "isError": False}


def _pair_path(path):
    d, f = os.path.split(path)
    stem, ext = os.path.splitext(f)
    if "_air" in stem:
        other = stem.replace("_air", "_road", 1)
    elif "_road" in stem:
        other = stem.replace("_road", "_air", 1)
    else:
        return None
    p = os.path.join(d, other + ext)
    return p if os.path.isfile(p) else None


def tool_render_recipe(a):
    from concurrent.futures import ThreadPoolExecutor

    entries = a.get("entries")
    if not entries and a.get("recipe_file"):
        with open(a["recipe_file"], "r", encoding="utf-8") as f:
            data = json.load(f)
        entries = data.get("entries") if isinstance(data, dict) else data
    if not entries:
        raise RuntimeError("entries 또는 recipe_file 이 필요합니다.")
    out = a["out"]
    os.makedirs(out, exist_ok=True)

    # pair 확장: 짝 파일에 같은 애니메이션/시점을 적용한 항목을 추가한다 (이미 있는 항목은 건드리지 않음).
    expanded, seen = [], set()
    for e in entries:
        key = (os.path.normpath(e["path"]), e.get("skin"))
        seen.add(key)
    for e in entries:
        expanded.append(e)
        if e.get("pair"):
            other = _pair_path(e["path"])
            if other and (os.path.normpath(other), e.get("skin")) not in seen:
                pe = {k: v for k, v in e.items() if k not in ("name", "pair")}
                pe["path"] = other
                expanded.append(pe)

    def one(e):
        stem = os.path.splitext(os.path.basename(e["path"]))[0]
        skin = e.get("skin")
        args = ["frame", e["path"]]
        if skin == "all":
            args += ["--out", out, "--skin", "all"]
        else:
            name = e.get("name") or (stem + ("__" + skin if skin else ""))
            args += ["--out", os.path.join(out, name + ".png")]
            if skin:
                args += ["--skin", skin]
        for flag, val in (("--animation", e.get("animation")), ("--time", e.get("time")),
                          ("--scale", a.get("scale")), ("--atlas", e.get("atlas"))):
            if val is not None:
                args.extend([flag, str(val)])
        if a.get("keep_shadow"):
            args.append("--keep-shadow")
        if a.get("crop") is False:
            args.append("--no-crop")
        if a.get("pma") is False:
            args.append("--no-pma")
        try:
            r = run_cli(args)
        except Exception as ex:
            return {"path": e["path"], "ok": False, "error": str(ex)}
        if r.get("ok") and r.get("results"):
            return {"path": e["path"], "ok": True, "outputs": [
                {"output": x["output"], "animation": x["animation"], "time": x["time"],
                 "width": x["width"], "height": x["height"], "skin": x.get("skin")} for x in r["results"]]}
        reason = (r.get("skipped") or [{}])[0].get("reason") or r.get("error") or "알 수 없는 오류"
        return {"path": e["path"], "ok": False, "error": reason}

    with ThreadPoolExecutor(max_workers=max(1, int(a.get("workers", 4)))) as ex:
        results = list(ex.map(one, expanded))
    done = [r for r in results if r["ok"]]
    summary = {"ok": True, "requested": len(expanded), "rendered": sum(len(r["outputs"]) for r in done),
               "failed": [r for r in results if not r["ok"]], "results": done}
    content = [{"type": "text", "text": json.dumps(summary, ensure_ascii=False, indent=1)}]
    if a.get("preview", False):
        attach_images(content, [o["output"] for r in done for o in r["outputs"]])
    return {"content": content, "isError": False}


def call_tool(name, a):
    if name == "spine_contact_sheet":
        return tool_contact_sheet(a)
    if name == "spine_render_recipe":
        return tool_render_recipe(a)
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
