use crate::models::TrackerEvent;
use std::sync::atomic::AtomicU64;
use tokio::sync::Mutex;

// Simple file-based event store with sequential IDs
pub struct EventStore {
    events: Mutex<Vec<TrackerEvent>>,
    next_id: AtomicU64,
}

impl EventStore {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // In a real implementation, this would load from disk
        // For now, we'll use in-memory but structured for easy disk persistence
        Ok(Self {
            events: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        })
    }
    
    pub async fn add_event(&self, mut event: TrackerEvent) -> Result<u64, Box<dyn std::error::Error>> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        event.id = id;
        
        // In a real implementation, this would append to a disk file
        // For now, we'll use a mutex-protected vector
        let mut events = self.events.lock().await;
        events.push(event);
        
        Ok(id)
    }
    
    pub async fn get_events_paginated(&self, page: usize, page_size: usize) -> Result<Vec<TrackerEvent>, Box<dyn std::error::Error>> {
        let events = self.events.lock().await;
        let start = page * page_size;
        let end = std::cmp::min(start + page_size, events.len());
        Ok(events[start..end].to_vec())
    }
}