use crate::mapper::TechAcronymMapper;
use crate::models::{AsrHistoryProvider, AsrResult, AsrService, TranscriptionEntry, TtsService};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

pub struct TranscriptionProcessor {
    mapper: TechAcronymMapper,
    history: Arc<dyn AsrHistoryProvider>,
    tts_service: Arc<dyn TtsService>,
    confidence_threshold: f32,
    enabled: bool,
    output_tx: broadcast::Sender<TranscriptionEntry>,
}

impl TranscriptionProcessor {
    pub fn new(
        mapper: TechAcronymMapper,
        history: Arc<dyn AsrHistoryProvider>,
        tts_service: Arc<dyn TtsService>,
    ) -> Self {
        let (output_tx, _output_rx) = broadcast::channel(128);
        Self {
            mapper,
            history,
            tts_service,
            confidence_threshold: 0.7,
            enabled: true,
            output_tx,
        }
    }

    pub fn subscribe_processed_results(&self) -> broadcast::Receiver<TranscriptionEntry> {
        self.output_tx.subscribe()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub async fn handle_result(
        &mut self,
        result: AsrResult,
        fallback_asr: Option<&mut dyn AsrService>,
        is_fallback: bool,
    ) -> anyhow::Result<Option<TranscriptionEntry>> {
        if !self.enabled || !result.is_final {
            return Ok(None);
        }

        if result.confidence < self.confidence_threshold && !is_fallback {
            self.tts_service.yell("stop droning on like a slob")?;
            if let Some(fallback) = fallback_asr {
                fallback.start_real_time_session()?;
            }
            return Ok(None);
        }

        let mapped_text = self.mapper.map(&result.text);
        let entry = TranscriptionEntry {
            id: Uuid::new_v4().to_string(),
            text: mapped_text,
            confidence: result.confidence,
            timestamp: current_timestamp_millis(),
            is_fallback,
            metadata: HashMap::from([("originalText".to_owned(), result.text)]),
        };

        self.history.add_entry(entry.clone()).await;
        let _ = self.output_tx.send(entry.clone());
        Ok(Some(entry))
    }
}

fn current_timestamp_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
