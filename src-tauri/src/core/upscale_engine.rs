use std::{path::Path, process::Command};

use super::{
    engine::{DirectoryCliEngine, EnhancementEngine},
    models::ModelManifest,
    realcugan::RealCuganNcnnEngine,
    realesrgan::RealEsrganNcnnEngine,
};

pub enum NcnnUpscaleEngine {
    RealEsrgan(RealEsrganNcnnEngine),
    RealCugan(RealCuganNcnnEngine),
}

impl NcnnUpscaleEngine {
    pub fn from_model(
        model: ModelManifest,
        tile_size: u32,
        gpu_id: Option<u32>,
        tta: bool,
    ) -> Result<Self, String> {
        match model.engine.as_str() {
            "realesrgan-ncnn-vulkan" => {
                let mut engine = RealEsrganNcnnEngine::new("realesrgan-ncnn-vulkan", model)
                    .map_err(|error| error.to_string())?
                    .with_tile_size(tile_size)
                    .map_err(|error| error.to_string())?;
                if let Some(gpu_id) = gpu_id {
                    engine = engine.with_gpu_id(gpu_id);
                }
                Ok(Self::RealEsrgan(engine.with_tta(tta)))
            }
            "realcugan-ncnn-vulkan" => {
                let mut engine = RealCuganNcnnEngine::new("realcugan-ncnn-vulkan", model)
                    .map_err(|error| error.to_string())?
                    .with_tile_size(tile_size)
                    .map_err(|error| error.to_string())?;
                if let Some(gpu_id) = gpu_id {
                    engine = engine.with_gpu_id(gpu_id);
                }
                Ok(Self::RealCugan(engine.with_tta(tta)))
            }
            other => Err(format!("Unsupported bounded NCNN upscale engine: {other}")),
        }
    }

    pub fn self_test(&self) -> Result<(), String> {
        match self {
            Self::RealEsrgan(engine) => engine.self_test().map_err(|error| error.to_string()),
            Self::RealCugan(engine) => engine.self_test().map_err(|error| error.to_string()),
        }
    }

    pub fn build_command(&self, input: &Path, output: &Path) -> Command {
        match self {
            Self::RealEsrgan(engine) => engine.build_directory_command(input, output),
            Self::RealCugan(engine) => engine.build_directory_command(input, output),
        }
    }
}
