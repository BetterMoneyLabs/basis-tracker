use anyhow::Result;
use basis_cli_lib::{
    account::AccountManager,
    api::{ReserveTokenConfig, TrackerClient},
    config::ConfigManager,
};
use basis_core::acceptance::AcceptanceConfig;
use basis_store::ExtendedReserveInfo;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// TUI-specific configuration including acceptance policy and address book
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TuiConfig {
    pub server_url: String,
    pub current_account: Option<String>,
    #[serde(default = "AcceptanceConfig::default_collateral")]
    pub acceptance: AcceptanceConfig,
    #[serde(default)]
    pub address_book: HashMap<String, String>,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:3048".to_string(),
            current_account: None,
            acceptance: AcceptanceConfig::default_collateral(),
            address_book: HashMap::new(),
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
        let mut config_path =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
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

    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)?;
        std::fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn update_acceptance(&mut self, config: AcceptanceConfig) -> Result<()> {
        self.config.acceptance = config;
        self.save()
    }

    pub fn update_address_book(&mut self, address_book: HashMap<String, String>) -> Result<()> {
        self.config.address_book = address_book;
        self.save()
    }

    pub fn set_server_url(&mut self, server_url: String) -> Result<()> {
        self.config.server_url = server_url;
        self.save()
    }

    pub fn set_current_account(&mut self, current_account: Option<String>) -> Result<()> {
        self.config.current_account = current_account;
        self.save()
    }
}

pub enum Screen {
    MainMenu,
    Accounts,
    Notes,
    Reserves,
    AddressBook,
    Settings,
    CreateNote,
    RedeemNote,
    CreateReserve,
    AcceptancePolicy,
    TrackerHealth,
    EmergencyRedeem,
}

pub struct App {
    pub screen: Screen,
    pub account_manager: AccountManager,
    pub client: TrackerClient,
    pub server_url: String,
    pub current_account: Option<AccountInfo>,
    /// Set when a default account was auto-created on first run; the intro
    /// screen displays its public key to the user before the main menu.
    pub intro_account: Option<AccountInfo>,
    pub reserve_status: Option<ReserveInfo>,
    pub reserve_token_config: Option<ReserveTokenConfig>,
    pub issued_notes: Vec<NoteInfo>,
    pub received_notes: Vec<NoteInfo>,
    pub notification: Option<(String, bool)>,
    pub running: bool,
    pub server_connected: bool,
    pub address_book: HashMap<String, String>,
    pub acceptance_config: AcceptanceConfig,
    pub policy_uploaded: bool,
    pub tui_config_manager: TuiConfigManager,
}

pub struct _ReserveCache {
    pub _reserves: HashMap<String, ExtendedReserveInfo>,
    pub _last_updated: Instant,
    pub _ttl: Duration,
}

impl _ReserveCache {
    pub fn _new() -> Self {
        Self {
            _reserves: HashMap::new(),
            _last_updated: Instant::now(),
            _ttl: Duration::from_secs(30 * 60), // 30 minutes
        }
    }

    pub fn _is_stale(&self) -> bool {
        self._last_updated.elapsed() > self._ttl
    }

    pub fn _get_reserve(&self, pubkey: &str) -> Option<&ExtendedReserveInfo> {
        self._reserves.get(pubkey)
    }

    pub fn _update(&mut self, reserves: HashMap<String, ExtendedReserveInfo>) {
        self._reserves = reserves;
        self._last_updated = Instant::now();
    }
}

#[derive(Clone)]
pub struct AccountInfo {
    pub name: String,
    pub pubkey: String,
    pub _created_at: u64,
}

#[derive(Clone)]
pub struct NoteInfo {
    pub issuer: String,
    pub recipient: String,
    pub amount: u64,
    pub redeemed: u64,
    pub _timestamp: u64,
}

#[derive(Clone)]
pub struct ReserveInfo {
    pub issuer: String,
    pub total_debt: u64,
    pub collateral: u64,
    pub ratio: f64,
    pub note_count: usize,
    pub _last_updated: u64,
    pub has_pending_refund: bool,
    pub reserve_token_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WalletStats {
    pub total_assets: u64,
    pub total_liabilities: u64,
    pub net_position: i64,
    pub coverage_ratio: Option<f64>,
    pub asset_note_count: usize,
    pub liability_note_count: usize,
}

pub fn compute_wallet_stats(
    issued_notes: &[NoteInfo],
    received_notes: &[NoteInfo],
    reserve_status: Option<&ReserveInfo>,
) -> WalletStats {
    let total_assets: u64 = received_notes
        .iter()
        .map(|n| n.amount.saturating_sub(n.redeemed))
        .sum();
    let total_liabilities: u64 = issued_notes
        .iter()
        .map(|n| n.amount.saturating_sub(n.redeemed))
        .sum();
    let net_position = total_assets as i64 - total_liabilities as i64;
    let coverage_ratio = reserve_status.map(|r| r.ratio);

    WalletStats {
        total_assets,
        total_liabilities,
        net_position,
        coverage_ratio,
        asset_note_count: received_notes.len(),
        liability_note_count: issued_notes.len(),
    }
}

impl App {
    pub fn compute_stats(&self) -> WalletStats {
        compute_wallet_stats(
            &self.issued_notes,
            &self.received_notes,
            self.reserve_status.as_ref(),
        )
    }

    pub async fn new() -> Result<Self> {
        let config_manager = ConfigManager::new(None)?;
        let mut account_manager = AccountManager::new(config_manager.clone())?;

        // First run: no accounts exist yet — auto-create a default account so
        // the wallet is usable immediately. The intro screen will show its
        // public key to the user.
        let intro_account = if account_manager.accounts.is_empty() {
            let account = account_manager.create_account("default")?;
            Some(AccountInfo {
                name: account.name.clone(),
                pubkey: account.get_pubkey_hex(),
                _created_at: account.created_at,
            })
        } else {
            None
        };

        let server_url = config_manager.get_config().server_url.clone();
        let client = TrackerClient::new(server_url.clone());

        let current_account = account_manager.get_current().map(|acc| AccountInfo {
            name: acc.name.clone(),
            pubkey: acc.get_pubkey_hex(),
            _created_at: acc.created_at,
        });

        let mut address_book = HashMap::new();

        // Auto-populate address book with existing accounts (accounts are source of truth)
        for account in account_manager.accounts.values() {
            address_book.insert(account.name.clone(), account.get_pubkey_hex());
        }

        // Load TUI config (acceptance policy and address book)
        let tui_config_manager = TuiConfigManager::new()?;
        let acceptance_config = tui_config_manager.get_config().acceptance.clone();
        let saved_address_book = tui_config_manager.get_config().address_book.clone();
        address_book.extend(saved_address_book);

        let mut app = Self {
            screen: Screen::MainMenu,
            account_manager,
            client,
            server_url,
            current_account,
            intro_account,
            reserve_status: None,
            reserve_token_config: None,
            issued_notes: Vec::new(),
            received_notes: Vec::new(),
            notification: None,
            running: true,
            server_connected: false,
            address_book,
            acceptance_config,
            policy_uploaded: false,
            tui_config_manager,
        };

        app.refresh_data().await?;
        Ok(app)
    }

    pub async fn refresh_data(&mut self) -> Result<()> {
        self.server_connected = self.client.health_check().await.unwrap_or(false);

        // Refresh reserve-token configuration from the tracker
        self.reserve_token_config = self.client.get_reserve_token_config().await.ok();

        // Refresh reserve status
        if let Some(ref acc) = self.current_account {
            if let Ok(status) = self.client.get_reserve_status(&acc.pubkey).await {
                self.reserve_status = Some(ReserveInfo {
                    issuer: status.issuer_pubkey,
                    total_debt: status.total_debt,
                    collateral: status.collateral,
                    ratio: status.collateralization_ratio,
                    note_count: status.note_count,
                    _last_updated: status.last_updated,
                    has_pending_refund: status.has_pending_refund,
                    reserve_token_id: status.reserve_token_id,
                });
            }

            // Refresh notes
            if let Ok(notes) = self.client.get_issuer_notes(&acc.pubkey).await {
                self.issued_notes = notes
                    .into_iter()
                    .map(|n| NoteInfo {
                        issuer: n.issuer_pubkey,
                        recipient: n.recipient_pubkey,
                        amount: n.amount_collected,
                        redeemed: n.amount_redeemed,
                        _timestamp: n.timestamp,
                    })
                    .collect();
            }

            if let Ok(notes) = self.client.get_recipient_notes(&acc.pubkey).await {
                self.received_notes = notes
                    .into_iter()
                    .map(|n| NoteInfo {
                        issuer: n.issuer_pubkey,
                        recipient: n.recipient_pubkey,
                        amount: n.amount_collected,
                        redeemed: n.amount_redeemed,
                        _timestamp: n.timestamp,
                    })
                    .collect();
            }
        }
        Ok(())
    }

    pub fn set_notification(&mut self, message: String, is_error: bool) {
        self.notification = Some((message, is_error));
    }

    pub fn navigate_to(&mut self, screen: Screen) {
        self.screen = screen;
        // Don't clear notification here - let it be displayed on the next screen
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(amount: u64) -> NoteInfo {
        NoteInfo {
            issuer: "issuer".to_string(),
            recipient: "recipient".to_string(),
            amount,
            redeemed: 0,
            _timestamp: 0,
        }
    }

    fn reserve(ratio: f64) -> ReserveInfo {
        ReserveInfo {
            issuer: "issuer".to_string(),
            total_debt: 0,
            collateral: 0,
            ratio,
            note_count: 0,
            _last_updated: 0,
            has_pending_refund: false,
            reserve_token_id: None,
        }
    }

    #[test]
    fn stats_empty() {
        let stats = compute_wallet_stats(&[], &[], None);
        assert_eq!(stats.total_assets, 0);
        assert_eq!(stats.total_liabilities, 0);
        assert_eq!(stats.net_position, 0);
        assert_eq!(stats.coverage_ratio, None);
        assert_eq!(stats.asset_note_count, 0);
        assert_eq!(stats.liability_note_count, 0);
    }

    #[test]
    fn stats_assets_greater_than_liabilities() {
        let issued = vec![note(1_000_000_000), note(2_000_000_000)];
        let received = vec![note(5_000_000_000)];
        let stats = compute_wallet_stats(&issued, &received, None);
        assert_eq!(stats.total_assets, 5_000_000_000);
        assert_eq!(stats.total_liabilities, 3_000_000_000);
        assert_eq!(stats.net_position, 2_000_000_000);
        assert_eq!(stats.asset_note_count, 1);
        assert_eq!(stats.liability_note_count, 2);
    }

    #[test]
    fn stats_liabilities_greater_than_assets() {
        let issued = vec![note(5_000_000_000)];
        let received = vec![note(1_000_000_000)];
        let stats = compute_wallet_stats(&issued, &received, None);
        assert_eq!(stats.net_position, -4_000_000_000);
    }

    #[test]
    fn stats_coverage_ratio_passed_through() {
        let issued = vec![note(1_000_000_000)];
        let r = reserve(0.95);
        let stats = compute_wallet_stats(&issued, &[], Some(&r));
        assert!((stats.coverage_ratio.unwrap() - 0.95).abs() < 1e-6);
    }

    #[test]
    fn stats_zero_liabilities_with_reserve() {
        let r = reserve(1.5);
        let stats = compute_wallet_stats(&[], &[], Some(&r));
        assert_eq!(stats.total_liabilities, 0);
        assert!((stats.coverage_ratio.unwrap() - 1.5).abs() < 1e-6);
    }

    fn note_with_redeemed(amount: u64, redeemed: u64) -> NoteInfo {
        NoteInfo {
            issuer: "issuer".to_string(),
            recipient: "recipient".to_string(),
            amount,
            redeemed,
            _timestamp: 0,
        }
    }

    #[test]
    fn stats_use_outstanding_amounts() {
        // 5 ERG issued, 2 ERG redeemed = 3 ERG liability
        // 4 ERG received, 1 ERG redeemed = 3 ERG asset
        let issued = vec![note_with_redeemed(5_000_000_000, 2_000_000_000)];
        let received = vec![note_with_redeemed(4_000_000_000, 1_000_000_000)];
        let stats = compute_wallet_stats(&issued, &received, None);

        assert_eq!(stats.total_liabilities, 3_000_000_000);
        assert_eq!(stats.total_assets, 3_000_000_000);
        assert_eq!(stats.net_position, 0);
        // Note counts still reflect total notes, not outstanding notes.
        assert_eq!(stats.liability_note_count, 1);
        assert_eq!(stats.asset_note_count, 1);
    }

    #[test]
    fn stats_fully_redeemed_note_contributes_zero() {
        let issued = vec![note_with_redeemed(3_000_000_000, 3_000_000_000)];
        let received = vec![note_with_redeemed(2_000_000_000, 2_000_000_000)];
        let stats = compute_wallet_stats(&issued, &received, None);

        assert_eq!(stats.total_liabilities, 0);
        assert_eq!(stats.total_assets, 0);
        assert_eq!(stats.net_position, 0);
    }
}
