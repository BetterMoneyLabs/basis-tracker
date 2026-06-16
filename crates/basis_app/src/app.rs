use anyhow::Result;
use basis_cli_lib::{
    account::{Account, AccountManager},
    api::TrackerClient,
    config::ConfigManager,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use basis_core::acceptance::AcceptanceConfig;
use basis_store::ExtendedReserveInfo;

/// TUI-specific configuration including acceptance policy
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TuiConfig {
    pub server_url: String,
    pub current_account: Option<String>,
    #[serde(default = "AcceptanceConfig::default_collateral")]
    pub acceptance: AcceptanceConfig,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:3048".to_string(),
            current_account: None,
            acceptance: AcceptanceConfig::default_collateral(),
        }
    }
}

/// Manages TUI-specific configuration file at ~/.basis/ui.toml
#[derive(Debug, Clone)]
pub struct TuiConfigManager {
    config_path: std::path::PathBuf,
    config: TuiConfig,
}

impl TuiConfigManager {
    pub fn new() -> Result<Self> {
        let mut config_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        config_path.push(".basis");
        std::fs::create_dir_all(&config_path)?;
        config_path.push("ui.toml");

        let config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            TuiConfig::default()
        };

        Ok(Self {
            config_path,
            config,
        })
    }

    pub fn get_config(&self) -> &TuiConfig {
        &self.config
    }

    pub fn get_config_mut(&mut self) -> &mut TuiConfig {
        &mut self.config
    }

    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn update_acceptance(&mut self, config: AcceptanceConfig) -> Result<()> {
        self.config.acceptance = config;
        self.save()
    }

    pub fn get_acceptance(&self) -> &AcceptanceConfig {
        &self.config.acceptance
    }
}

pub enum Screen {
    MainMenu,
    Accounts,
    Notes,
    Reserves,
    Transactions,
    AddressBook,
    Settings,
    CreateNote,
    RedeemNote,
    CreateReserve,
    GenerateTransaction,
    AcceptancePolicy,
}

pub struct App {
    pub screen: Screen,
    pub account_manager: AccountManager,
    pub client: TrackerClient,
    pub server_url: String,
    pub current_account: Option<AccountInfo>,
    pub reserve_status: Option<ReserveInfo>,
    pub issued_notes: Vec<NoteInfo>,
    pub received_notes: Vec<NoteInfo>,
    pub notification: Option<(String, bool)>,
    pub running: bool,
    pub server_connected: bool,
    pub address_book: HashMap<String, String>,
    pub acceptance_config: AcceptanceConfig,
    pub reserve_cache: Option<ReserveCache>,
    pub policy_uploaded: bool,
    pub tui_config_manager: TuiConfigManager,
}

pub struct ReserveCache {
    pub reserves: HashMap<String, ExtendedReserveInfo>,
    pub last_updated: Instant,
    pub ttl: Duration,
}

impl ReserveCache {
    pub fn new() -> Self {
        Self {
            reserves: HashMap::new(),
            last_updated: Instant::now(),
            ttl: Duration::from_secs(30 * 60), // 30 minutes
        }
    }

    pub fn is_stale(&self) -> bool {
        self.last_updated.elapsed() > self.ttl
    }

    pub fn get_reserve(&self, pubkey: &str) -> Option<&ExtendedReserveInfo> {
        self.reserves.get(pubkey)
    }

    pub fn update(&mut self, reserves: HashMap<String, ExtendedReserveInfo>) {
        self.reserves = reserves;
        self.last_updated = Instant::now();
    }
}

#[derive(Clone)]
pub struct AccountInfo {
    pub name: String,
    pub pubkey: String,
    pub created_at: u64,
}

#[derive(Clone)]
pub struct NoteInfo {
    pub issuer: String,
    pub recipient: String,
    pub amount: u64,
    pub redeemed: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
pub struct ReserveInfo {
    pub issuer: String,
    pub total_debt: u64,
    pub collateral: u64,
    pub ratio: f64,
    pub note_count: usize,
    pub last_updated: u64,
}

impl App {
    pub async fn new() -> Result<Self> {
        let config_manager = ConfigManager::new(None)?;
        let account_manager = AccountManager::new(config_manager.clone())?;
        let server_url = config_manager.get_config().server_url.clone();
        let client = TrackerClient::new(server_url.clone());

        let current_account = account_manager.get_current().map(|acc| AccountInfo {
            name: acc.name.clone(),
            pubkey: acc.get_pubkey_hex(),
            created_at: acc.created_at,
        });

        let mut address_book = HashMap::new();
        // Add demo contacts with correct pubkeys
        address_book.insert(
            "bob".to_string(),
            "03af13e39dd0ccc7429f9dfa5a056b71a8f5160eaf179763a03e0b55d8feec2cea".to_string(),
        );
        address_book.insert(
            "charlie".to_string(),
            "02a3b5c7d9e1f3a5b7c9d1e3f5a7b9c1d3e5f7a9b1c3d5e7f9a1b3c5d7e9f1a3b5c".to_string(),
        );

        // Load TUI config (acceptance policy)
        let tui_config_manager = TuiConfigManager::new()?;
        let acceptance_config = tui_config_manager.get_config().acceptance.clone();

        let mut app = Self {
            screen: Screen::MainMenu,
            account_manager,
            client,
            server_url,
            current_account,
            reserve_status: None,
            issued_notes: Vec::new(),
            received_notes: Vec::new(),
            notification: None,
            running: true,
            server_connected: false,
            address_book,
            acceptance_config,
            reserve_cache: None,
            policy_uploaded: false,
            tui_config_manager,
        };

        app.refresh_data().await?;
        Ok(app)
    }

    pub async fn refresh_data(&mut self) -> Result<()> {
        self.server_connected = self.client.health_check().await.unwrap_or(false);

        // Refresh reserve status
        if let Some(ref acc) = self.current_account {
            match self.client.get_reserve_status(&acc.pubkey).await {
                Ok(status) => {
                    self.reserve_status = Some(ReserveInfo {
                        issuer: status.issuer_pubkey,
                        total_debt: status.total_debt,
                        collateral: status.collateral,
                        ratio: status.collateralization_ratio,
                        note_count: status.note_count,
                        last_updated: status.last_updated,
                    });
                }
                Err(_) => {}
            }

            // Refresh notes
            match self.client.get_issuer_notes(&acc.pubkey).await {
                Ok(notes) => {
                    self.issued_notes = notes
                        .into_iter()
                        .map(|n| NoteInfo {
                            issuer: n.issuer_pubkey,
                            recipient: n.recipient_pubkey,
                            amount: n.amount_collected,
                            redeemed: n.amount_redeemed,
                            timestamp: n.timestamp,
                        })
                        .collect();
                }
                Err(_) => {}
            }

            match self.client.get_recipient_notes(&acc.pubkey).await {
                Ok(notes) => {
                    self.received_notes = notes
                        .into_iter()
                        .map(|n| NoteInfo {
                            issuer: n.issuer_pubkey,
                            recipient: n.recipient_pubkey,
                            amount: n.amount_collected,
                            redeemed: n.amount_redeemed,
                            timestamp: n.timestamp,
                        })
                        .collect();
                }
                Err(_) => {}
            }
        }
        Ok(())
    }

    pub fn set_notification(&mut self, message: String, is_error: bool) {
        self.notification = Some((message, is_error));
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    pub fn navigate_to(&mut self, screen: Screen) {
        self.screen = screen;
        // Don't clear notification here - let it be displayed on the next screen
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}
