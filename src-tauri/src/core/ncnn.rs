use serde::Serialize;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::models::{bundled_model_catalog, ModelManifest};

#[derive(Debug, Clone, Serialize)]
pub struct BinaryProbe {
    pub name: String,
    pub available: bool,
    pub detail: Option<String>,
    pub resolved_path: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelProbe {
    pub id: String,
    pub available: bool,
    pub resolved_path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NcnnRuntimeInfo {
    pub realesrgan: BinaryProbe,
    pub realcugan: BinaryProbe,
    pub rife: BinaryProbe,
    /// Model profiles that have a real on-disk `.param` + `.bin` payload pair.
    /// Keeping this field runtime-ready makes older frontends fail closed instead of
    /// enabling an AI button merely because a manifest entry exists.
    pub models: Vec<ModelManifest>,
    /// Complete manifest catalog for diagnostics and newer frontends.
    pub model_catalog: Vec<ModelManifest>,
    pub model_probes: Vec<ModelProbe>,
    pub model_dir: Option<String>,
}

fn executable_name(program: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        if program.to_ascii_lowercase().ends_with(".exe") {
            program.to_string()
        } else {
            format!("{program}.exe")
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        program.to_string()
    }
}

fn push_runtime_root(roots: &mut Vec<(PathBuf, String)>, root: PathBuf, source: &str) {
    if !roots.iter().any(|(existing, _)| existing == &root) {
        roots.push((root, source.to_string()));
    }
}

fn runtime_roots() -> Vec<(PathBuf, String)> {
    let mut roots = Vec::new();

    if let Ok(root) = env::var("CLE_VIDEOSR_RUNTIME_DIR") {
        push_runtime_root(&mut roots, PathBuf::from(root), "CLE_VIDEOSR_RUNTIME_DIR");
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            push_runtime_root(&mut roots, parent.join("runtime"), "adjacent runtime");
            push_runtime_root(
                &mut roots,
                parent.join("resources").join("runtime"),
                "adjacent resources",
            );

            if let Some(contents) = parent.parent() {
                push_runtime_root(
                    &mut roots,
                    contents.join("Resources").join("runtime"),
                    "macOS Resources",
                );
            }
        }
    }

    roots
}

fn binary_candidates(program: &str) -> Vec<(PathBuf, String)> {
    let file_name = executable_name(program);
    let mut candidates = Vec::new();

    for (root, source) in runtime_roots() {
        candidates.push((root.join("bin").join(&file_name), source.clone()));
        candidates.push((root.join(&file_name), source));
    }

    candidates.push((PathBuf::from(program), "PATH".to_string()));
    candidates
}

fn first_output_line(output: &std::process::Output) -> Option<String> {
    String::from_utf8(output.stdout.clone())
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| !line.trim().is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            String::from_utf8(output.stderr.clone()).ok().and_then(|text| {
                text.lines()
                    .find(|line| !line.trim().is_empty())
                    .map(str::to_owned)
            })
        })
}

fn probe_binary(program: &str) -> BinaryProbe {
    let mut last_error = None;

    for (candidate, source) in binary_candidates(program) {
        match Command::new(&candidate).arg("-h").output() {
            Ok(output) => {
                return BinaryProbe {
                    name: program.to_string(),
                    available: true,
                    detail: first_output_line(&output),
                    resolved_path: Some(candidate.to_string_lossy().into_owned()),
                    source,
                };
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    BinaryProbe {
        name: program.to_string(),
        available: false,
        detail: last_error,
        resolved_path: None,
        source: "unresolved".to_string(),
    }
}

pub fn resolve_ncnn_binary(program: &str) -> PathBuf {
    for (candidate, _) in binary_candidates(program) {
        if Command::new(&candidate).arg("-h").output().is_ok() {
            return candidate;
        }
    }
    PathBuf::from(program)
}

fn model_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = env::var("CLE_VIDEOSR_MODEL_DIR") {
        candidates.push(PathBuf::from(path));
    }

    for (root, _) in runtime_roots() {
        candidates.push(root.join("models"));
    }

    candidates
}

pub fn resolve_ncnn_model_dir() -> Option<PathBuf> {
    model_dir_candidates()
        .into_iter()
        .find(|candidate| candidate.is_dir())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn direct_model_pair(directory: &Path, prefix: Option<&str>) -> bool {
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };

    let mut params = HashSet::new();
    let mut bins = HashSet::new();

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if prefix.is_some_and(|required| !stem.starts_with(required)) {
            continue;
        }

        if extension.eq_ignore_ascii_case("param") {
            params.insert(stem.to_string());
        } else if extension.eq_ignore_ascii_case("bin") {
            bins.insert(stem.to_string());
        }
    }

    params.iter().any(|stem| bins.contains(stem))
}

fn find_model_pair_directory(
    directory: &Path,
    prefix: Option<&str>,
    remaining_depth: usize,
) -> Option<PathBuf> {
    if !directory.is_dir() {
        return None;
    }
    if direct_model_pair(directory, prefix) {
        return Some(directory.to_path_buf());
    }
    if remaining_depth == 0 {
        return None;
    }

    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let child = entry.path();
        if child.is_dir() {
            if let Some(found) =
                find_model_pair_directory(&child, prefix, remaining_depth.saturating_sub(1))
            {
                return Some(found);
            }
        }
    }
    None
}

fn model_payload_candidates(model: &ModelManifest, binary: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let is_realesrgan = model.engine == "realesrgan-ncnn-vulkan";

    for root in model_dir_candidates() {
        if is_realesrgan {
            push_unique_path(&mut candidates, root.clone());
            push_unique_path(&mut candidates, root.join(&model.model_stem));
        } else {
            if root
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value == model.model_stem)
            {
                push_unique_path(&mut candidates, root.clone());
            }
            push_unique_path(&mut candidates, root.join(&model.model_stem));
        }
    }

    if let Ok(current) = env::current_dir() {
        if is_realesrgan {
            push_unique_path(&mut candidates, current.join("models"));
            push_unique_path(&mut candidates, current.join("runtime").join("models"));
        } else {
            push_unique_path(&mut candidates, current.join(&model.model_stem));
            push_unique_path(
                &mut candidates,
                current.join("models").join(&model.model_stem),
            );
            push_unique_path(
                &mut candidates,
                current
                    .join("runtime")
                    .join("models")
                    .join(&model.model_stem),
            );
        }
    }

    if let Some(parent) = binary.and_then(Path::parent) {
        if is_realesrgan {
            push_unique_path(&mut candidates, parent.join("models"));
            push_unique_path(&mut candidates, parent.to_path_buf());
            push_unique_path(&mut candidates, parent.join(&model.model_stem));
        } else {
            push_unique_path(&mut candidates, parent.join(&model.model_stem));
            push_unique_path(
                &mut candidates,
                parent.join("models").join(&model.model_stem),
            );
        }
    }

    candidates
}

pub fn resolve_ncnn_model_payload_dir(
    model: &ModelManifest,
    binary: Option<&Path>,
) -> Option<PathBuf> {
    let prefix = (model.engine == "realesrgan-ncnn-vulkan").then_some(model.model_stem.as_str());
    let depth = if prefix.is_some() { 2 } else { 1 };

    model_payload_candidates(model, binary)
        .into_iter()
        .find_map(|candidate| find_model_pair_directory(&candidate, prefix, depth))
}

fn probe_model(model: &ModelManifest) -> ModelProbe {
    let binary = resolve_ncnn_binary(&model.engine);
    let resolved = resolve_ncnn_model_payload_dir(model, Some(&binary));

    match resolved {
        Some(path) => ModelProbe {
            id: model.id.clone(),
            available: true,
            resolved_path: Some(path.to_string_lossy().into_owned()),
            detail: "model .param/.bin payload pair detected".into(),
        },
        None => ModelProbe {
            id: model.id.clone(),
            available: false,
            resolved_path: None,
            detail: format!(
                "missing model payload for {} ({})",
                model.display_name, model.model_stem
            ),
        },
    }
}

pub fn detect_ncnn_runtime() -> NcnnRuntimeInfo {
    let model_catalog = bundled_model_catalog()
        .map(|catalog| catalog.models)
        .unwrap_or_default();
    let model_probes = model_catalog.iter().map(probe_model).collect::<Vec<_>>();
    let ready_ids = model_probes
        .iter()
        .filter(|probe| probe.available)
        .map(|probe| probe.id.as_str())
        .collect::<HashSet<_>>();
    let models = model_catalog
        .iter()
        .filter(|model| ready_ids.contains(model.id.as_str()))
        .cloned()
        .collect();
    let model_dir = resolve_ncnn_model_dir().map(|path| path.to_string_lossy().into_owned());

    NcnnRuntimeInfo {
        realesrgan: probe_binary("realesrgan-ncnn-vulkan"),
        realcugan: probe_binary("realcugan-ncnn-vulkan"),
        rife: probe_binary("rife-ncnn-vulkan"),
        models,
        model_catalog,
        model_probes,
        model_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::{direct_model_pair, find_model_pair_directory};
    use std::{fs, path::PathBuf};

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "c-le-videosr-ncnn-test-{}-{}",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn requires_matching_param_and_bin_pair() {
        let root = test_root("pair");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("demo.param"), b"param").unwrap();
        assert!(!direct_model_pair(&root, Some("demo")));
        fs::write(root.join("demo.bin"), b"bin").unwrap();
        assert!(direct_model_pair(&root, Some("demo")));
        assert!(!direct_model_pair(&root, Some("other")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_nested_payload_directory() {
        let root = test_root("nested");
        let nested = root.join("rife-v4.6");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("flownet.param"), b"param").unwrap();
        fs::write(nested.join("flownet.bin"), b"bin").unwrap();
        assert_eq!(find_model_pair_directory(&root, None, 2), Some(nested));
        let _ = fs::remove_dir_all(&root);
    }
}
