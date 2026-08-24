use serde::Serialize;
use std::process::Command;

use super::models::{bundled_model_catalog, ModelManifest};

#[derive(Debug, Clone, Serialize)]
pub struct BinaryProbe {
    pub name: String,
    pub available: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NcnnRuntimeInfo {
    pub realesrgan: BinaryProbe,
    pub realcugan: BinaryProbe,
    pub rife: BinaryProbe,
    pub models: Vec<ModelManifest>,
}

fn probe_binary(program: &str) -> BinaryProbe {
    match Command::new(program).arg("-h").output() {
        Ok(output) => {
            let detail = String::from_utf8(output.stdout)
                .ok()
                .and_then(|text| text.lines().next().map(str::to_owned))
                .or_else(|| {
                    String::from_utf8(output.stderr)
                        .ok()
                        .and_then(|text| text.lines().next().map(str::to_owned))
                });

            BinaryProbe {
                name: program.to_string(),
                available: true,
                detail,
            }
        }
        Err(error) => BinaryProbe {
            name: program.to_string(),
            available: false,
            detail: Some(error.to_string()),
        },
    }
}

pub fn detect_ncnn_runtime() -> NcnnRuntimeInfo {
    let models = bundled_model_catalog()
        .map(|catalog| catalog.models)
        .unwrap_or_default();

    NcnnRuntimeInfo {
        realesrgan: probe_binary("realesrgan-ncnn-vulkan"),
        realcugan: probe_binary("realcugan-ncnn-vulkan"),
        rife: probe_binary("rife-ncnn-vulkan"),
        models,
    }
}
