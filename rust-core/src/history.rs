use crate::models::{AsrHistoryProvider, TranscriptionEntry};
use async_trait::async_trait;
use tokio::sync::{RwLock, watch};

pub struct InMemoryAsrHistory {
    entries: RwLock<Vec<TranscriptionEntry>>,
    tx: watch::Sender<Vec<TranscriptionEntry>>,
}

impl InMemoryAsrHistory {
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(Vec::new());
        Self {
            entries: RwLock::new(Vec::new()),
            tx,
        }
    }

    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
        let _ = self.tx.send(entries.clone());
    }

    pub async fn update_entry(&self, id: &str, new_text: &str) {
        let mut entries = self.entries.write().await;
        for entry in entries.iter_mut() {
            if entry.id == id {
                entry.text = new_text.to_owned();
            }
        }
        let _ = self.tx.send(entries.clone());
    }
}

impl Default for InMemoryAsrHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsrHistoryProvider for InMemoryAsrHistory {
    fn history_stream(&self) -> watch::Receiver<Vec<TranscriptionEntry>> {
        self.tx.subscribe()
    }

    async fn add_entry(&self, entry: TranscriptionEntry) {
        let mut entries = self.entries.write().await;
        entries.insert(0, entry);
        if entries.len() > 1000 {
            entries.truncate(1000);
        }
        let _ = self.tx.send(entries.clone());
    }

    async fn search(&self, query: &str) -> Vec<TranscriptionEntry> {
        let entries = self.entries.read().await;
        if query.trim().is_empty() {
            return entries.clone();
        }

        let query = query.to_ascii_lowercase();
        entries
            .iter()
            .filter(|entry| {
                entry.text.to_ascii_lowercase().contains(&query)
                    || entry
                        .metadata
                        .values()
                        .any(|value| value.to_ascii_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }
}
