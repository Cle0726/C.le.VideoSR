use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub schema_version: u32,
    pub models: Vec<ModelManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub id: String,
    pub display_name: String,
    pub task: String,
    pub engine: String,
    pub scale: u32,
    #[serde(default)]
    pub frame_multiplier: Option<u32>,
    pub content: String,
    pub model_stem: String,
    pub bundled: bool,
    pub license_status: String,
}

pub fn bundled_model_catalog() -> Result<ModelCatalog, String> {
    serde_json::from_str(include_str!("../../../models/manifest.json"))
        .map_err(|error| format!("Unable to parse bundled model manifest: {error}"))
}
