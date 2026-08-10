use crate::models::TrackerEvent;
use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use tokio::sync::Mutex;

pub const MAX_STORED_EVENTS: usize = 10_000;
pub const MAX_EVENT_PAGE_SIZE: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("event identifier space exhausted")]
    IdentifierExhausted,
    #[error("page_size must be between 1 and {MAX_EVENT_PAGE_SIZE}")]
    InvalidPageSize,
    #[error("event pagination offset overflow")]
    PaginationOverflow,
}

// Simple file-based event store with sequential IDs.
pub struct EventStore {
    events: Mutex<VecDeque<TrackerEvent>>,
    next_id: AtomicU64,
}

impl EventStore {
    pub async fn new() -> Result<Self, EventStoreError> {
        // In a real implementation, this would load from disk. The in-memory
        // fallback is deliberately bounded so a public event stream cannot
        // consume process memory without limit.
        Ok(Self {
            events: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn add_event(&self, mut event: TrackerEvent) -> Result<u64, EventStoreError> {
        let id = self
            .next_id
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |next| next.checked_add(1),
            )
            .map_err(|_| EventStoreError::IdentifierExhausted)?;
        event.id = id;

        let mut events = self.events.lock().await;
        if events.len() == MAX_STORED_EVENTS {
            events.pop_front();
        }
        events.push_back(event);
        Ok(id)
    }

    pub async fn get_events_paginated(
        &self,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<TrackerEvent>, EventStoreError> {
        if !(1..=MAX_EVENT_PAGE_SIZE).contains(&page_size) {
            return Err(EventStoreError::InvalidPageSize);
        }
        let events = self.events.lock().await;
        let start = page
            .checked_mul(page_size)
            .ok_or(EventStoreError::PaginationOverflow)?;
        if start >= events.len() {
            return Ok(Vec::new());
        }
        Ok(events.iter().skip(start).take(page_size).cloned().collect())
    }

    pub async fn get_recent_events(
        &self,
        limit: usize,
    ) -> Result<Vec<TrackerEvent>, EventStoreError> {
        if !(1..=MAX_EVENT_PAGE_SIZE).contains(&limit) {
            return Err(EventStoreError::InvalidPageSize);
        }
        let events = self.events.lock().await;
        let start = events.len().saturating_sub(limit);
        Ok(events.iter().skip(start).cloned().collect())
    }

    /// Create an in-memory event store for testing.
    pub fn new_in_memory() -> Self {
        Self {
            events: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EventType;

    fn event(timestamp: u64) -> TrackerEvent {
        TrackerEvent {
            id: 0,
            event_type: EventType::Commitment,
            timestamp,
            issuer_pubkey: None,
            recipient_pubkey: None,
            amount: None,
            reserve_box_id: None,
            collateral_amount: None,
            redeemed_amount: None,
            height: None,
        }
    }

    #[tokio::test]
    async fn pagination_rejects_zero_oversized_and_overflowing_requests() {
        let store = EventStore::new_in_memory();
        assert!(matches!(
            store.get_events_paginated(0, 0).await,
            Err(EventStoreError::InvalidPageSize)
        ));
        assert!(matches!(
            store.get_events_paginated(0, MAX_EVENT_PAGE_SIZE + 1).await,
            Err(EventStoreError::InvalidPageSize)
        ));
        assert!(matches!(
            store.get_events_paginated(usize::MAX, 2).await,
            Err(EventStoreError::PaginationOverflow)
        ));
    }

    #[tokio::test]
    async fn event_retention_is_bounded_and_recent_reads_use_the_tail() {
        let store = EventStore::new_in_memory();
        for timestamp in 0..=(MAX_STORED_EVENTS as u64) {
            store.add_event(event(timestamp)).await.unwrap();
        }

        let first = store.get_events_paginated(0, 1).await.unwrap();
        assert_eq!(first[0].timestamp, 1);
        let recent = store.get_recent_events(2).await.unwrap();
        assert_eq!(
            recent
                .iter()
                .map(|event| event.timestamp)
                .collect::<Vec<_>>(),
            vec![MAX_STORED_EVENTS as u64 - 1, MAX_STORED_EVENTS as u64]
        );
    }

    #[tokio::test]
    async fn out_of_range_pages_are_empty_without_panicking() {
        let store = EventStore::new_in_memory();
        store.add_event(event(1)).await.unwrap();
        assert!(store
            .get_events_paginated(100, 10)
            .await
            .unwrap()
            .is_empty());
    }
}
