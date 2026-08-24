use serde::Serialize;
use std::{env, path::PathBuf, process::Command};

#[derive(Debug, Clone, Serialize)]
pub struct MediaRuntimeInfo {
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    pub ffmpeg_version: Option<String>,
    pub libx264: bool,
    pub libx265: bool,
    pub managed_bin_dir: Option<String>,
}

fn push_unique(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

fn runtime_bin_candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(root) = env::var("CLE_VIDEOSR_RUNTIME_DIR") {
        push_unique(&mut dirs, PathBuf::from(root).join("bin"));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            push_unique(&mut dirs, parent.join("runtime").join("bin"));
            push_unique(&mut dirs, parent.join("resources").join("runtime").join("bin"));
            if let Some(contents) = parent.parent() {
                push_unique(
                    &mut dirs,
                    contents.join("Resources").join("runtime").join("bin"),
                );
            }
        }
    }

    dirs
}

pub fn configure_managed_runtime_path() -> Option<PathBuf> {
    let managed = runtime_bin_candidates().into_iter().find(|dir| dir.is_dir())?;
    let current = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![managed.clone()];
    paths.extend(env::split_paths(&current));
    if let Ok(joined) = env::join_paths(paths) {
        env::set_var("PATH", joined);
    }
    Some(managed)
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn detect_media_runtime() -> MediaRuntimeInfo {
    let managed_bin_dir = runtime_bin_candidates()
        .into_iter()
        .find(|dir| dir.is_dir())
        .map(|path| path.to_string_lossy().into_owned());

    let version_output = Command::new("ffmpeg").arg("-version").output().ok();
    let ffmpeg_available = version_output
        .as_ref()
        .map(|output| output.status.success())
        .unwrap_or(false);

    let ffmpeg_version = version_output.and_then(|output| {
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout)
            .ok()
            .and_then(|text| text.lines().next().map(str::to_owned))
    });

    let encoders = if ffmpeg_available {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .unwrap_or_default()
    } else {
        String::new()
    };

    MediaRuntimeInfo {
        ffmpeg_available,
        ffprobe_available: command_available("ffprobe"),
        ffmpeg_version,
        libx264: encoders.contains("libx264"),
        libx265: encoders.contains("libx265"),
        managed_bin_dir,
    }
}
