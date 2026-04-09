use crate::models::{AsrHistoryProvider, TranscriptionEntry};
use std::sync::Arc;

pub struct SearchModel {
    history: Arc<dyn AsrHistoryProvider>,
    query: String,
    results: Vec<TranscriptionEntry>,
}

impl SearchModel {
    pub fn new(history: Arc<dyn AsrHistoryProvider>) -> Self {
        Self {
            history,
            query: String::new(),
            results: Vec::new(),
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn results(&self) -> &[TranscriptionEntry] {
        &self.results
    }

    pub async fn on_query_changed(&mut self, new_query: impl Into<String>) {
        self.query = new_query.into();
        self.results = self.history.search(&self.query).await;
    }
}
