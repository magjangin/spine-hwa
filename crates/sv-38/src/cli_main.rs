// 콘솔 서브시스템 바이너리: stdout으로 JSON 결과를 내보내는 헤드리스 CLI (AI/MCP 호출용).
extern crate spine_sys_38;

fn main() {
    let code = spine_viewer_core::headless::run(std::env::args().collect());
    std::process::exit(code);
}
