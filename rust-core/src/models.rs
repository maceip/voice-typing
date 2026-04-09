use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone)]
pub struct AsrResult {
    pub text: String,
    pub confidence: f32,
    pub is_final: bool,
}

#[derive(Debug, Clone)]
pub struct TranscriptionEntry {
    pub id: String,
    pub text: String,
    pub confidence: f32,
    pub timestamp: i64,
    pub is_fallback: bool,
    pub metadata: HashMap<String, String>,
}

#[async_trait]
pub trait AsrHistoryProvider: Send + Sync {
    fn history_stream(&self) -> watch::Receiver<Vec<TranscriptionEntry>>;
    async fn add_entry(&self, entry: TranscriptionEntry);
    async fn search(&self, query: &str) -> Vec<TranscriptionEntry>;
}

#[async_trait]
pub trait AsrService: Send + Sync {
    async fn initialize(&mut self, model_path: &str) -> anyhow::Result<()>;
    fn start_real_time_session(&mut self) -> anyhow::Result<()>;
    fn stop_real_time_session(&mut self) -> anyhow::Result<()>;
    fn subscribe_results(&self) -> broadcast::Receiver<AsrResult>;
}

#[async_trait]
pub trait WakeWordService: Send + Sync {
    fn listening_stream(&self) -> watch::Receiver<bool>;
    fn subscribe_wake_word_detected(&self) -> broadcast::Receiver<()>;
    fn start_listening(&mut self, wake_word: &str) -> anyhow::Result<()>;
    fn stop_listening(&mut self) -> anyhow::Result<()>;
}

pub trait TtsService: Send + Sync {
    fn yell(&self, message: &str) -> anyhow::Result<()>;
}
