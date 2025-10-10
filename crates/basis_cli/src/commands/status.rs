use crate::api::TrackerClient;
use anyhow::Result;

pub async fn handle_status_command(client: &TrackerClient) -> Result<()> {
    // Check server health
    let is_healthy = client.health_check().await?;
    
    if is_healthy {
        println!("✅ Server is healthy");
    } else {
        println!("❌ Server is not responding");
        return Ok(());
    }
    
    // Get recent events
    let events = client.get_recent_events().await?;
    
    println!("\nRecent Events (last {}):", events.len());
    for event in events {
        let event_summary = match event.event_type.as_str() {
            "NoteUpdated" => {
                if let (Some(issuer), Some(recipient), Some(amount)) = (
                    event.issuer_pubkey,
                    event.recipient_pubkey,
                    event.amount,
                ) {
                    format!("Note: {} -> {} ({} nanoERG)", 
                        &issuer[..16], &recipient[..16], amount)
                } else {
                    "Note updated".to_string()
                }
            }
            "ReserveCreated" => {
                if let (Some(_issuer), Some(reserve_id), Some(collateral)) = (
                    event.issuer_pubkey,
                    event.reserve_box_id,
                    event.collateral_amount,
                ) {
                    format!("Reserve created: {} ({} nanoERG)", 
                        &reserve_id[..16], collateral)
                } else {
                    "Reserve created".to_string()
                }
            }
            "ReserveToppedUp" => {
                if let (Some(reserve_id), Some(collateral)) = (
                    event.reserve_box_id,
                    event.collateral_amount,
                ) {
                    format!("Reserve topped up: {} (+{} nanoERG)", 
                        &reserve_id[..16], collateral)
                } else {
                    "Reserve topped up".to_string()
                }
            }
            "ReserveRedeemed" => {
                if let (Some(reserve_id), Some(redeemed)) = (
                    event.reserve_box_id,
                    event.redeemed_amount,
                ) {
                    format!("Reserve redeemed: {} (-{} nanoERG)", 
                        &reserve_id[..16], redeemed)
                } else {
                    "Reserve redeemed".to_string()
                }
            }
            "ReserveSpent" => {
                if let Some(reserve_id) = event.reserve_box_id {
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
            "CollateralAlert" => {
                "Collateral alert".to_string()
            }
            _ => {
                format!("{} event", event.event_type)
            }
        };
        
        println!("  [{}] {} - {}", 
            event.timestamp, 
            event_summary,
            event.height.map(|h| format!("height {}", h)).unwrap_or_default()
        );
    }
    
    Ok(())
}