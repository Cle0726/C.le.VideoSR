use serde::Serialize;
use std::{
    env,
    path::PathBuf,
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
pub struct NcnnRuntimeInfo {
    pub realesrgan: BinaryProbe,
    pub realcugan: BinaryProbe,
    pub rife: BinaryProbe,
    pub model_dir: Option<String>,
    pub models: Vec<ModelManifest>,
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
        .and_then(|text| text.lines().find(|line| !line.trim().is_empty()).map(str::to_owned))
        .or_else(|| {
            String::from_utf8(output.stderr.clone())
                .ok()
                .and_then(|text| text.lines().find(|line| !line.trim().is_empty()).map(str::to_owned))
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
    model_dir_candidates().into_iter().find(|candidate| candidate.is_dir())
}

pub fn detect_ncnn_runtime() -> NcnnRuntimeInfo {
    let models = bundled_model_catalog()
        .map(|catalog| catalog.models)
        .unwrap_or_default();
    let model_dir = resolve_ncnn_model_dir().map(|path| path.to_string_lossy().into_owned());

    NcnnRuntimeInfo {
        realesrgan: probe_binary("realesrgan-ncnn-vulkan"),
        realcugan: probe_binary("realcugan-ncnn-vulkan"),
        rife: probe_binary("rife-ncnn-vulkan"),
        model_dir,
        models,
    }
}
