use crate::api::{TrackerClient, TrackerEvent};
use anyhow::Result;
use serde::Serialize;

/// A single recent tracker event, pre-rendered to its human summary text.
#[derive(Debug, Serialize)]
pub struct StatusEventSummary {
    pub timestamp: u64,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
}

/// Server health plus recent tracker events (`status` command).
#[derive(Debug, Serialize)]
pub struct ServerStatusResult {
    pub healthy: bool,
    pub recent_events: Vec<StatusEventSummary>,
}

/// Check server health and fetch recent events.
pub async fn get_server_status(client: &TrackerClient) -> Result<ServerStatusResult> {
    let is_healthy = client.health_check().await?;
    if !is_healthy {
        return Ok(ServerStatusResult {
            healthy: false,
            recent_events: Vec::new(),
        });
    }

    let events = client.get_recent_events().await?;
    let recent_events = events.into_iter().map(summarize_event).collect();

    Ok(ServerStatusResult {
        healthy: true,
        recent_events,
    })
}

fn summarize_event(event: TrackerEvent) -> StatusEventSummary {
    let summary = match event.event_type.as_str() {
        "NoteUpdated" => {
            if let (Some(issuer), Some(recipient), Some(amount)) = (
                event.issuer_pubkey.as_ref(),
                event.recipient_pubkey.as_ref(),
                event.amount,
            ) {
                format!(
                    "Note: {} -> {} ({} nanoERG)",
                    &issuer[..16],
                    &recipient[..16],
                    amount
                )
            } else {
                "Note updated".to_string()
            }
        }
        "ReserveCreated" => {
            if let (Some(_issuer), Some(reserve_id), Some(collateral)) = (
                event.issuer_pubkey.as_ref(),
                event.reserve_box_id.as_ref(),
                event.collateral_amount,
            ) {
                format!(
                    "Reserve created: {} ({} nanoERG)",
                    &reserve_id[..16],
                    collateral
                )
            } else {
                "Reserve created".to_string()
            }
        }
        "ReserveToppedUp" => {
            if let (Some(reserve_id), Some(collateral)) =
                (event.reserve_box_id.as_ref(), event.collateral_amount)
            {
                format!(
                    "Reserve topped up: {} (+{} nanoERG)",
                    &reserve_id[..16],
                    collateral
                )
            } else {
                "Reserve topped up".to_string()
            }
        }
        "ReserveRedeemed" => {
            if let (Some(reserve_id), Some(redeemed)) =
                (event.reserve_box_id.as_ref(), event.redeemed_amount)
            {
                format!(
                    "Reserve redeemed: {} (-{} nanoERG)",
                    &reserve_id[..16],
                    redeemed
                )
            } else {
                "Reserve redeemed".to_string()
            }
        }
        "ReserveSpent" => {
            if let Some(reserve_id) = event.reserve_box_id.as_ref() {
                format!("Reserve spent: {}", &reserve_id[..16])
            } else {
                "Reserve spent".to_string()
            }
        }
        "Commitment" => {
            if let Some(height) = event.height {
                format!("State commitment at height {}", height)
            } else {
                "State commitment".to_string()
            }
        }
        "CollateralAlert" => "Collateral alert".to_string(),
        _ => {
            format!("{} event", event.event_type)
        }
    };

    StatusEventSummary {
        timestamp: event.timestamp,
        summary,
        height: event.height,
    }
}

pub async fn handle_status_command(client: &TrackerClient, json: bool) -> Result<()> {
    let status = get_server_status(client).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    if !status.healthy {
        println!("❌ Server is not responding");
        return Ok(());
    }

    println!("✅ Server is healthy");

    println!("\nRecent Events (last {}):", status.recent_events.len());
    for event in &status.recent_events {
        println!(
            "  [{}] {} - {}",
            event.timestamp,
            event.summary,
            event
                .height
                .map(|h| format!("height {}", h))
                .unwrap_or_default()
        );
    }

    Ok(())
}
