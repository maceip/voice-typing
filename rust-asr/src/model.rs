use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WiredModelPaths {
    pub model_dir: PathBuf,
    pub vad_model: PathBuf,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CurrentWiredModel;

impl CurrentWiredModel {
    pub const MODEL_DIR: &'static str =
        "assets/models/sherpa-onnx-moonshine-base-en-quantized-2026-02-27";
    pub const VAD_PATH: &'static str = "assets/models/silero_vad.onnx";
    pub const SAMPLE_RATE: u32 = 16_000;

    pub fn locate_from(root: impl AsRef<Path>) -> Result<WiredModelPaths> {
        let root = root.as_ref();
        let model_dir = root.join(Self::MODEL_DIR);
        let vad_model = root.join(Self::VAD_PATH);

        if !model_dir.exists() {
            anyhow::bail!("model directory not found: {}", model_dir.display());
        }
        if !vad_model.exists() {
            anyhow::bail!("VAD model not found: {}", vad_model.display());
        }

        for required in ["encoder_model.ort", "decoder_model_merged.ort", "tokens.txt"] {
            let path = model_dir.join(required);
            path.metadata()
                .with_context(|| format!("missing current wired model file: {}", path.display()))?;
        }

        Ok(WiredModelPaths {
            model_dir,
            vad_model,
            sample_rate: Self::SAMPLE_RATE,
        })
    }
}
