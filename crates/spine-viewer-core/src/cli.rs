use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub atlas_path: PathBuf,
    pub data_path: PathBuf,
    pub is_json: bool,
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn is_data_ext(ext: &str) -> bool {
    ext == "skel" || ext == "json"
}

/// 선택/드롭된 파일들로부터 atlas + 데이터(.skel/.json) 쌍을 결정한다.
/// 한쪽만 주어지면 같은 폴더의 같은 이름(확장자만 다른) 파일을 찾는다.
pub fn resolve_paths(paths: &[PathBuf]) -> Result<CliArgs, String> {
    let mut atlas = paths.iter().find(|p| ext_of(p) == "atlas").cloned();
    let mut data = paths.iter().find(|p| is_data_ext(&ext_of(p))).cloned();

    if atlas.is_none() {
        if let Some(d) = &data {
            let cand = d.with_extension("atlas");
            if cand.is_file() {
                atlas = Some(cand);
            }
        }
    }
    if data.is_none() {
        if let Some(a) = &atlas {
            data = ["skel", "json"]
                .iter()
                .map(|e| a.with_extension(e))
                .find(|c| c.is_file());
        }
    }

    match (atlas, data) {
        (Some(atlas_path), Some(data_path)) => {
            let is_json = ext_of(&data_path) == "json";
            Ok(CliArgs { atlas_path, data_path, is_json })
        }
        (None, Some(_)) => Err("Atlas file not found. Select the .atlas file together with the .skel/.json file.".to_string()),
        (Some(_), None) => Err("Skeleton data not found. Select the .skel/.json file together with the .atlas file.".to_string()),
        (None, None) => Err("Select a .skel/.json file and its .atlas file.".to_string()),
    }
}

impl CliArgs {
    /// 인자가 없으면 `Ok(None)` (빈 창으로 시작해 앱 안에서 파일을 연다).
    pub fn parse() -> Result<Option<Self>, String> {
        let args: Vec<String> = std::env::args().collect();
        let bin_name = args.first().cloned().unwrap_or_else(|| "sv-X.X.exe".to_string());

        if args.len() == 1 {
            return Ok(None);
        }

        if args.len() == 2 {
            // exe 위에 파일 하나를 드롭한 경우: 같은 이름의 짝 파일을 찾는다
            return resolve_paths(&[PathBuf::from(&args[1])]).map(Some);
        }

        if args.len() == 3 {
            // Binary mode: argv[1] = atlas, argv[2] = skel
            return Ok(Some(Self {
                atlas_path: PathBuf::from(&args[1]),
                data_path: PathBuf::from(&args[2]),
                is_json: false,
            }));
        }

        if args.len() == 4 {
            let flag = &args[2];
            if flag == "--json" || flag == "-j" {
                return Ok(Some(Self {
                    atlas_path: PathBuf::from(&args[1]),
                    data_path: PathBuf::from(&args[3]),
                    is_json: true,
                }));
            } else {
                return Err(format!("Usage: {} [<atlas-path> [--json|-j] <skel-path>]", bin_name));
            }
        }

        // argc > 4: Original spec §2 (binary mode with argv[1], argv[2])
        Ok(Some(Self {
            atlas_path: PathBuf::from(&args[1]),
            data_path: PathBuf::from(&args[2]),
            is_json: false,
        }))
    }
}
