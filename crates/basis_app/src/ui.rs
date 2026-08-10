use crate::acceptance_policy::{
    create_policy, get_blacklist_entries, get_policy_summary, get_whitelist_entries,
    get_whitelist_entries_with_limit, remove_from_blacklist, remove_from_whitelist,
};
use crate::app::{App, NoteInfo, Screen, WalletStats};
use anyhow::Result;
use basis_offchain::signing::{add_input_proof, redemption_signing_message};
use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::Transaction;
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;
use ergo_lib::wallet::secret_key::SecretKey;
use std::io::{self, Write};

// ANSI Color codes
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const CYAN: &str = "\x1b[36m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const _MAGENTA: &str = "\x1b[35m";
pub const WHITE: &str = "\x1b[37m";
pub const GRAY: &str = "\x1b[90m";

pub async fn run(app: &mut App) -> Result<()> {
    clear_screen();
    if app.intro_account.is_some() {
        draw_intro(app);
    } else {
        print_banner();
    }
    wait_for_enter("Press Enter to continue...");

    while app.running {
        clear_screen();
        draw_header(app);
        draw_notification(app);

        match app.screen {
            Screen::MainMenu => draw_main_menu(app).await?,
            Screen::Accounts => draw_accounts(app).await?,
            Screen::AddressBook => draw_address_book(app).await?,
            Screen::Notes => draw_notes(app).await?,
            Screen::Reserves => draw_reserves(app).await?,
            Screen::Settings => draw_settings(app).await?,
            Screen::CreateNote => draw_create_note(app).await?,
            Screen::RedeemNote => draw_redeem_note(app).await?,
            Screen::CreateReserve => draw_create_reserve(app).await?,
            Screen::AcceptancePolicy => draw_acceptance_policy(app).await?,
            Screen::TrackerHealth => draw_tracker_health(app).await?,
            Screen::EmergencyRedeem => draw_emergency_redeem(app).await?,
        }
    }

    clear_screen();
    println!("{}Goodbye!{}", CYAN, RESET);
    Ok(())
}

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
    io::stdout().flush().unwrap();
}

fn print_banner() {
    println!();
    println!("{}██████╗  █████╗ ███████╗██╗███████╗{}", CYAN, RESET);
    println!("{}██╔══██╗██╔══██╗██╔════╝██║██╔════╝{}", CYAN, RESET);
    println!("{}██████╔╝███████║███████╗██║███████╗{}", CYAN, RESET);
    println!("{}██╔══██╗██╔══██║╚════██║██║╚════██║{}", CYAN, RESET);
    println!("{}██████╔╝██║  ██║███████║██║███████║{}", CYAN, RESET);
    println!("{}╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝╚══════╝{}", CYAN, RESET);
    println!();
    println!("{}        Wallet v0.1.0{}", GRAY, RESET);
    println!();
    println!("{}  Free Banking For Everyone{}", RED, RESET);
    println!("{}  Interactive Terminal Basis Wallet{}", GRAY, RESET);
    println!();
}

/// First-run intro screen: shown when a default account was auto-created,
/// informing the user about their new public key.
fn draw_intro(app: &App) {
    print_banner();
    if let Some(ref acc) = app.intro_account {
        println!("{}  WELCOME TO BASIS WALLET{}", BOLD, RESET);
        println!("{}  ────────────────────────{}\n", CYAN, RESET);
        println!("  A new account has been created for you:\n");
        println!("    {}Name:{}      {}", BOLD, RESET, acc.name);
        println!("    {}Public Key:{}", BOLD, RESET);
        println!("    {}{}{}\n", GREEN, acc.pubkey, RESET);
        println!("  Share this public key to receive notes (IOUs) from others.");
        println!(
            "  You can manage your accounts anytime in {}Settings → Accounts Management{}.",
            CYAN, RESET
        );
        println!();
        println!(
            "  {}⚠ Back up your private key (Settings → Accounts Management → Export).{}",
            YELLOW, RESET
        );
        println!();
    }
}

fn draw_header(app: &App) {
    println!(
        "{}═══════════════════════════════════════════════════════════════{}",
        CYAN, RESET
    );
    print!("{}  BASIS Wallet{}", BOLD, RESET);

    if let Some(ref acc) = app.current_account {
        print!("{} | Account: {}{}", GRAY, GREEN, acc.name);
    } else {
        print!("{} | Account: {}none", GRAY, YELLOW);
    }

    print!("{} | Server: {}", GRAY, RESET);
    if app.server_connected {
        print!("{}● connected", GREEN);
    } else {
        print!("{}○ disconnected", RED);
    }

    if app.server_connected && !app.policy_uploaded {
        print!("{} | {}⚠ policy not uploaded{}", GRAY, YELLOW, RESET);
    }

    println!("{}", RESET);
    println!(
        "{}═══════════════════════════════════════════════════════════════{}\n",
        CYAN, RESET
    );
}

fn draw_notification(app: &App) {
    if let Some((ref msg, is_error)) = app.notification {
        let color = if is_error { RED } else { GREEN };
        let icon = if is_error { "✗" } else { "✓" };
        println!("{} {} {}{}{}\n", color, icon, BOLD, msg, RESET);
    }
}

fn draw_wallet_stats(stats: &WalletStats) {
    println!("{}  WALLET STATS{}", BOLD, RESET);
    println!("{}  ────────────{}", CYAN, RESET);

    println!(
        "  {}Assets:{}        {:.6} ERG ({} notes)",
        BOLD,
        RESET,
        stats.total_assets as f64 / 1_000_000_000.0,
        stats.asset_note_count
    );
    println!(
        "  {}Liabilities:{}   {:.6} ERG ({} notes)",
        BOLD,
        RESET,
        stats.total_liabilities as f64 / 1_000_000_000.0,
        stats.liability_note_count
    );

    let net_erg = stats.net_position as f64 / 1_000_000_000.0;
    let net_color = if stats.net_position >= 0 { GREEN } else { RED };
    let net_sign = if stats.net_position >= 0 { "+" } else { "" };
    println!(
        "  {}Net position:{}  {}{}{:.6} ERG{}",
        BOLD, RESET, net_color, net_sign, net_erg, RESET
    );

    match stats.coverage_ratio {
        None => {
            println!(
                "  {}Coverage:{}      {}N/A{} (no reserve)",
                BOLD, RESET, GRAY, RESET
            );
        }
        Some(_ratio) if stats.total_liabilities == 0 => {
            println!(
                "  {}Coverage:{}      {}N/A{} (no liabilities)",
                BOLD, RESET, GRAY, RESET
            );
        }
        Some(ratio) => {
            let percent = ratio * 100.0;
            let color = if ratio < 1.0 {
                RED
            } else if ratio < 1.2 {
                YELLOW
            } else if ratio < 1.5 {
                WHITE
            } else {
                GREEN
            };
            println!(
                "  {}Coverage:{}      {}{:.2}%{}",
                BOLD, RESET, color, percent, RESET
            );

            let warning = if ratio < 1.0 {
                Some((RED, "CRITICAL: liabilities covered below 100%"))
            } else if ratio < 1.2 {
                Some((YELLOW, "WARNING: liabilities covered below 120%"))
            } else if ratio < 1.5 {
                Some((CYAN, "CAUTION: liabilities covered below 150%"))
            } else {
                None
            };

            if let Some((color, msg)) = warning {
                println!("  {}⚠ {}{}", color, msg, RESET);
            }
        }
    }

    println!();
}

fn draw_wallet_stats_disconnected() {
    println!("{}  WALLET STATS{}", BOLD, RESET);
    println!("{}  ────────────{}", CYAN, RESET);
    println!(
        "  {}⚠ Server disconnected — stats unavailable{}\n",
        YELLOW, RESET
    );
}

async fn draw_main_menu(app: &mut App) -> Result<()> {
    println!("{}  MAIN MENU{}", BOLD, RESET);
    println!("{}  ─────────{}\n", CYAN, RESET);

    if app.server_connected {
        draw_wallet_stats(&app.compute_stats());
    } else {
        draw_wallet_stats_disconnected();
    }

    println!("  {}[1]{} Notes (IOU Assets & Liabilities)", CYAN, RESET);
    println!("  {}[2]{} My Reserves", CYAN, RESET);
    println!("  {}[3]{} My Acceptance Policy", CYAN, RESET);
    println!("  {}[4]{} Address Book", CYAN, RESET);
    println!("  {}[5]{} Settings", CYAN, RESET);
    println!();
    println!("  {}[r]{} Refresh Data", YELLOW, RESET);
    println!("  {}[q]{} Quit\n", RED, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => app.navigate_to(Screen::Notes),
        "2" => app.navigate_to(Screen::Reserves),
        "3" => app.navigate_to(Screen::AcceptancePolicy),
        "4" => app.navigate_to(Screen::AddressBook),
        "5" => app.navigate_to(Screen::Settings),
        "r" | "R" => {
            app.refresh_data().await?;
            if app.server_connected {
                app.set_notification("Server connected ✓".to_string(), false);
            } else {
                app.set_notification("Server disconnected ✗".to_string(), true);
            }
        }
        "q" | "Q" => app.quit(),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_accounts(app: &mut App) -> Result<()> {
    println!("{}  ACCOUNTS{}", BOLD, RESET);
    println!("{}  ─────────{}\n", CYAN, RESET);

    let accounts: Vec<_> = app
        .account_manager
        .list_accounts()
        .into_iter()
        .map(|a| a.clone())
        .collect();

    if accounts.is_empty() {
        println!("{}  No accounts found.{}\n", GRAY, RESET);
    } else {
        println!("  {}Available Accounts:{}", BOLD, RESET);
        for (i, account) in accounts.iter().enumerate() {
            let is_current = app
                .current_account
                .as_ref()
                .map(|acc| acc.name == account.name)
                .unwrap_or(false);

            if is_current {
                println!(
                    "  {}➤ [{}] {} {}(current){}",
                    GREEN,
                    i + 1,
                    account.name,
                    CYAN,
                    RESET
                );
            } else {
                println!("    [{}] {}", i + 1, account.name);
            }

            println!(
                "      {}Pubkey: {}...{}{}",
                GRAY,
                &account.get_pubkey_hex()[..16],
                &account.get_pubkey_hex()[56..66],
                RESET
            );
            println!();
        }
    }

    println!("  {}[c]{} Create Account", CYAN, RESET);
    println!("  {}[s]{} Switch Account", CYAN, RESET);
    println!("  {}[i]{} Import Account", CYAN, RESET);
    println!("  {}[e]{} Export Private Key", CYAN, RESET);
    println!("  {}[d]{} Delete Account", RED, RESET);
    println!();
    println!("  {}[b]{} Back to Settings\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "c" => {
            let name = read_input("Enter account name: ");
            if !name.is_empty() {
                match app.account_manager.create_account(&name) {
                    Ok(account) => {
                        let pubkey = account.get_pubkey_hex();
                        // Sync to address book
                        app.address_book.insert(name.clone(), pubkey.clone());
                        let _ = app
                            .tui_config_manager
                            .update_address_book(app.address_book.clone());
                        app.set_notification(format!("Created account '{}'", account.name), false);
                        app.current_account = Some(crate::app::AccountInfo {
                            name: account.name.clone(),
                            pubkey,
                            _created_at: account.created_at,
                        });
                        let _ = app
                            .tui_config_manager
                            .set_current_account(Some(account.name));
                    }
                    Err(e) => {
                        app.set_notification(format!("Error: {}", e), true);
                    }
                }
            }
        }
        "s" => {
            if !accounts.is_empty() {
                let idx_str = read_input("Enter account number: ");
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx > 0 && idx <= accounts.len() {
                        let name = accounts[idx - 1].name.clone();
                        match app.account_manager.switch_account(&name) {
                            Ok(_) => {
                                app.current_account = Some(crate::app::AccountInfo {
                                    name: accounts[idx - 1].name.clone(),
                                    pubkey: accounts[idx - 1].get_pubkey_hex(),
                                    _created_at: accounts[idx - 1].created_at,
                                });
                                let _ = app
                                    .tui_config_manager
                                    .set_current_account(Some(name.clone()));
                                app.set_notification(
                                    format!("Switched to account '{}'", name),
                                    false,
                                );
                                app.refresh_data().await?;
                            }
                            Err(e) => {
                                app.set_notification(format!("Error: {}", e), true);
                            }
                        }
                    } else {
                        app.set_notification("Invalid account number".to_string(), true);
                    }
                }
            }
        }
        "i" => {
            let name = read_input("Enter account name: ");
            let key = read_input("Enter private key (hex): ");
            if !name.is_empty() && !key.is_empty() {
                match basis_cli_lib::account::Account::from_private_key_hex(&name, &key) {
                    Ok(account) => {
                        let pubkey = account.get_pubkey_hex();
                        app.account_manager
                            .config_manager
                            .add_account(&name, &pubkey, &key)?;
                        // Sync to address book
                        app.address_book.insert(name.clone(), pubkey);
                        app.set_notification(format!("Imported account '{}'", name), false);
                    }
                    Err(e) => {
                        app.set_notification(format!("Error: {}", e), true);
                    }
                }
            }
        }
        "e" => {
            if let Some(ref acc) = app.current_account {
                if let Some(account) = app.account_manager.get_account(&acc.name) {
                    let key = account.get_private_key_hex();
                    println!("\n{}Private Key for '{}':{}", YELLOW, acc.name, RESET);
                    println!("{}\n", key);
                    wait_for_enter("Press Enter to continue...");
                }
            } else {
                app.set_notification("No account selected".to_string(), true);
            }
        }
        "d" => {
            if !accounts.is_empty() {
                let idx_str = read_input("Enter account number to delete: ");
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx > 0 && idx <= accounts.len() {
                        let confirm = read_input("Are you sure? (yes/no): ");
                        if confirm == "yes" {
                            let name = accounts[idx - 1].name.clone();
                            match app.account_manager.delete_account(&name) {
                                Ok(()) => {
                                    // Remove from address book and persist
                                    app.address_book.remove(&name);
                                    let _ = app
                                        .tui_config_manager
                                        .update_address_book(app.address_book.clone());

                                    // Clear current account if it was deleted
                                    if app
                                        .current_account
                                        .as_ref()
                                        .map(|a| a.name == name)
                                        .unwrap_or(false)
                                    {
                                        app.current_account = None;
                                        let _ = app.tui_config_manager.set_current_account(None);
                                    }

                                    app.set_notification(
                                        format!("Deleted account '{}'", name),
                                        false,
                                    );
                                }
                                Err(e) => {
                                    app.set_notification(format!("Error: {}", e), true);
                                }
                            }
                        }
                    } else {
                        app.set_notification("Invalid account number".to_string(), true);
                    }
                } else {
                    app.set_notification("Invalid account number".to_string(), true);
                }
            }
        }
        "b" | "B" => app.navigate_to(Screen::Settings),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_address_book(app: &mut App) -> Result<()> {
    println!("{}  ADDRESS BOOK{}", BOLD, RESET);
    println!("{}  ─────────────{}\n", CYAN, RESET);

    // Show accounts (read-only, synced from account manager)
    let accounts: Vec<_> = app
        .account_manager
        .list_accounts()
        .into_iter()
        .map(|a| a.clone())
        .collect();
    if !accounts.is_empty() {
        println!("  {}Accounts (auto-synced):{}", BOLD, RESET);
        for (i, account) in accounts.iter().enumerate() {
            let pubkey = account.get_pubkey_hex();
            println!(
                "  [{}] {}: {}...{} {}",
                i + 1,
                account.name,
                &pubkey[..16],
                &pubkey[56..66],
                GRAY
            );
        }
        println!();
    }

    // Show manual contacts
    let manual_contacts: Vec<_> = app
        .address_book
        .iter()
        .filter(|(name, _)| !accounts.iter().any(|a| &a.name == *name))
        .collect();

    if !manual_contacts.is_empty() {
        println!("  {}Additional Contacts:{}", BOLD, RESET);
        for (i, (name, pubkey)) in manual_contacts.iter().enumerate() {
            println!(
                "  [{}] {}: {}...{}",
                i + 1,
                name,
                &pubkey[..16],
                &pubkey[56..66]
            );
        }
        println!();
    } else if accounts.is_empty() {
        println!("{}  No contacts found.{}\n", GRAY, RESET);
    }

    println!("  {}[a]{} Add Contact", CYAN, RESET);
    println!("  {}[d]{} Delete Contact", RED, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "a" => {
            let name = read_input("Contact name: ");
            if !name.is_empty() {
                // Check if name conflicts with an account
                if let Some(account) = app.account_manager.get_account(&name) {
                    let account_pubkey = account.get_pubkey_hex();
                    app.set_notification(
                        format!(
                            "'{}' is an account with pubkey {}...{}",
                            name,
                            &account_pubkey[..16],
                            &account_pubkey[56..66]
                        ),
                        true,
                    );
                } else {
                    let pubkey = read_input("Public key (66 hex chars): ");
                    if pubkey.len() == 66 {
                        app.address_book.insert(name.clone(), pubkey);
                        let _ = app
                            .tui_config_manager
                            .update_address_book(app.address_book.clone());
                        app.set_notification(format!("Added contact '{}'", name), false);
                    } else {
                        app.set_notification(
                            "Invalid pubkey length (must be 66 hex chars)".to_string(),
                            true,
                        );
                    }
                }
            }
        }
        "d" => {
            if !app.address_book.is_empty() {
                let name = read_input("Contact name to delete: ");
                // Prevent deleting account entries from address book
                if app.account_manager.get_account(&name).is_some() {
                    app.set_notification(
                        format!(
                            "Cannot delete '{}' - it's an account. Delete from Accounts instead.",
                            name
                        ),
                        true,
                    );
                } else if app.address_book.remove(&name).is_some() {
                    let _ = app
                        .tui_config_manager
                        .update_address_book(app.address_book.clone());
                    app.set_notification(format!("Deleted contact '{}'", name), false);
                } else {
                    app.set_notification(format!("Contact '{}' not found", name), true);
                }
            }
        }
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_notes(app: &mut App) -> Result<()> {
    println!("{}  NOTES (IOU Assets & Liabilities){}", BOLD, RESET);
    println!("{}  ─────────────────────────────{}\n", CYAN, RESET);

    let issued_total_erg: f64 = app
        .issued_notes
        .iter()
        .map(|n| n.amount.saturating_sub(n.redeemed))
        .sum::<u64>() as f64
        / 1_000_000_000.0;
    let received_total_erg: f64 = app
        .received_notes
        .iter()
        .map(|n| n.amount.saturating_sub(n.redeemed))
        .sum::<u64>() as f64
        / 1_000_000_000.0;

    println!(
        "  {}[1]{} Notes Issued ({} notes, {:.6} ERG total liabilities)",
        CYAN,
        RESET,
        app.issued_notes.len(),
        issued_total_erg
    );
    println!(
        "  {}[2]{} Notes Received ({} notes, {:.6} ERG total assets)\n",
        CYAN,
        RESET,
        app.received_notes.len(),
        received_total_erg
    );

    println!("  {}[c]{} Create Note", CYAN, RESET);
    println!("  {}[r]{} Redeem Note", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => {
            println!("\n  {}Notes Issued:{}", BOLD, RESET);
            if app.issued_notes.is_empty() {
                println!("  {}None{}\n", GRAY, RESET);
            } else {
                for (i, note) in app.issued_notes.iter().enumerate() {
                    let outstanding = note.amount.saturating_sub(note.redeemed);
                    println!(
                        "  [{}] → {} | {} ERG ({} outstanding)",
                        i + 1,
                        &note.recipient[..16],
                        note.amount as f64 / 1_000_000_000.0,
                        outstanding as f64 / 1_000_000_000.0
                    );
                }
                println!();
            }
            wait_for_enter("Press Enter to continue...");
        }
        "2" => {
            println!("\n  {}Notes Received:{}", BOLD, RESET);
            if app.received_notes.is_empty() {
                println!("  {}None{}\n", GRAY, RESET);
            } else {
                for (i, note) in app.received_notes.iter().enumerate() {
                    let outstanding = note.amount.saturating_sub(note.redeemed);
                    println!(
                        "  [{}] ← {} | {} ERG ({} outstanding)",
                        i + 1,
                        &note.issuer[..16],
                        note.amount as f64 / 1_000_000_000.0,
                        outstanding as f64 / 1_000_000_000.0
                    );
                }
                println!();
            }
            wait_for_enter("Press Enter to continue...");
        }
        "c" => app.navigate_to(Screen::CreateNote),
        "r" => app.navigate_to(Screen::RedeemNote),
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_reserves(app: &mut App) -> Result<()> {
    println!("{}  MY RESERVES{}", BOLD, RESET);
    println!("{}  ───────────{}\n", CYAN, RESET);

    if let Some(ref reserve) = app.reserve_status {
        let ratio_color = ratio_color(reserve.ratio);
        let status = ratio_status(reserve.ratio);

        println!("  {}Issuer:{}", BOLD, RESET);
        println!(
            "  {}...{}\n",
            &reserve.issuer[..20],
            &reserve.issuer[46..56]
        );

        println!(
            "  {}Total Liabilities:{}     {} nanoERG ({:.6} ERG)",
            BOLD,
            RESET,
            reserve.total_debt,
            reserve.total_debt as f64 / 1_000_000_000.0
        );
        println!(
            "  {}Collateral:{}     {} nanoERG ({:.6} ERG)",
            BOLD,
            RESET,
            reserve.collateral,
            reserve.collateral as f64 / 1_000_000_000.0
        );
        println!(
            "  {}Ratio:{}          {}{}{}",
            BOLD, RESET, ratio_color, reserve.ratio, RESET
        );
        println!(
            "  {}Status:{}         {}{}{}",
            BOLD, RESET, ratio_color, status, RESET
        );
        if reserve.has_pending_refund {
            println!(
                "  {}⚠ Warning:{} Refund initiated for at least one reserve",
                YELLOW, RESET
            );
        }
        println!(
            "  {}Note Count:{}     {}\n",
            BOLD, RESET, reserve.note_count
        );

        // Visual bar
        let bar_width = 40;
        let filled = ((reserve.ratio / 3.0).min(1.0) * bar_width as f64) as usize;
        let bar: String = std::iter::repeat("█")
            .take(filled)
            .chain(std::iter::repeat("░").take(bar_width - filled))
            .collect();
        println!("  [{}{}{}]\n", ratio_color, bar, RESET);
    } else {
        println!("  {}No reserve data available.{}\n", GRAY, RESET);
    }

    println!("  {}[c]{} Create Reserve", CYAN, RESET);
    println!("  {}[r]{} Refresh Status", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "c" => app.navigate_to(Screen::CreateReserve),
        "r" => {
            app.refresh_data().await?;
            app.set_notification("Reserve status refreshed".to_string(), false);
        }
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_settings(app: &mut App) -> Result<()> {
    println!("{}  SETTINGS{}", BOLD, RESET);
    println!("{}  ─────────{}\n", CYAN, RESET);

    println!("  {}Tracker URL:{} {}", BOLD, RESET, app.server_url);
    println!();

    println!("  {}[1]{} Change Tracker URL", CYAN, RESET);
    println!("  {}[2]{} Accounts Management", CYAN, RESET);
    println!("  {}[3]{} Tracker Health", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => {
            let new_url = read_input("Enter new tracker URL: ");
            if !new_url.is_empty() {
                app.server_url = new_url.clone();
                app.client = basis_cli_lib::api::TrackerClient::new(new_url.clone());
                app.account_manager
                    .config_manager
                    .get_config_mut()
                    .server_url = new_url.clone();
                app.account_manager.config_manager.save()?;
                let _ = app.tui_config_manager.set_server_url(new_url.clone());
                app.set_notification(format!("Tracker URL updated to: {}", new_url), false);
            }
        }
        "2" => app.navigate_to(Screen::Accounts),
        "3" => app.navigate_to(Screen::TrackerHealth),
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_tracker_health(app: &mut App) -> Result<()> {
    println!("{}  TRACKER HEALTH{}", BOLD, RESET);
    println!("{}  ─────────────{}", CYAN, RESET);
    println!();

    if !app.server_connected {
        println!(
            "  {}⚠ Server disconnected — tracker health data unavailable.{}",
            YELLOW, RESET
        );
        println!();
        println!("  {}[b]{} Back to Settings\n", YELLOW, RESET);
        if read_choice("Select option: ").to_lowercase() == "b" {
            app.navigate_to(Screen::Settings);
        }
        return Ok(());
    }

    println!(
        "  {}Server status:{} {}● connected{}",
        BOLD, RESET, GREEN, RESET
    );
    println!();

    // Latest confirmed tracker box
    match app.client.get_latest_tracker_box_id().await {
        Ok(box_info) => {
            println!("  {}Latest tracker box:{}", BOLD, RESET);
            println!(
                "    ID:     {}...{}",
                &box_info.tracker_box_id[..16.min(box_info.tracker_box_id.len())],
                &box_info.tracker_box_id[box_info.tracker_box_id.len().saturating_sub(16)..]
            );
            println!("    Height: {}", box_info.height);
            println!();
        }
        Err(e) => {
            println!(
                "  {}⚠ Could not fetch latest tracker box: {}{}",
                YELLOW, e, RESET
            );
            println!();
        }
    }

    // Tracker state: local / confirmed / pending
    match app.client.get_tracker_state().await {
        Ok(state) => {
            println!("  {}Tracker state:{}", BOLD, RESET);
            println!(
                "    Local digest:     {}...{}",
                &state.local_digest[..16.min(state.local_digest.len())],
                &state.local_digest[state.local_digest.len().saturating_sub(16)..]
            );
            if let Some(ref digest) = state.confirmed_digest {
                println!(
                    "    Confirmed digest: {}...{}",
                    &digest[..16.min(digest.len())],
                    &digest[digest.len().saturating_sub(16)..]
                );
            }
            if let Some(height) = state.confirmed_height {
                println!("    Confirmed height: {}", height);
            }
            if let Some(ref box_id) = state.confirmed_box_id {
                println!(
                    "    Confirmed box:    {}...{}",
                    &box_id[..16.min(box_id.len())],
                    &box_id[box_id.len().saturating_sub(16)..]
                );
            }
            if let Some(ref tx_id) = state.pending_tx_id {
                println!(
                    "    Pending update:   {}...{}",
                    &tx_id[..16.min(tx_id.len())],
                    &tx_id[tx_id.len().saturating_sub(16)..]
                );
                if let Some(height) = state.pending_submitted_height {
                    println!("    Submitted height: {}", height);
                }
            } else {
                println!("    Pending update:   none");
            }
            println!();
        }
        Err(e) => {
            println!(
                "  {}⚠ Could not fetch tracker state: {}{}",
                YELLOW, e, RESET
            );
            println!();
        }
    }

    // Most recent event
    match app.client.get_recent_events().await {
        Ok(events) if !events.is_empty() => {
            let ev = &events[0];
            println!("  {}Most recent event:{}", BOLD, RESET);
            println!("    Type:      {}", ev.event_type);
            if let Some(height) = ev.height {
                println!("    Height:    {}", height);
            }
            println!("    Timestamp: {}", ev.timestamp);
            println!();
        }
        Ok(_) => {
            println!("  {}No recent events found.{}", GRAY, RESET);
            println!();
        }
        Err(e) => {
            println!(
                "  {}⚠ Could not fetch recent events: {}{}",
                YELLOW, e, RESET
            );
            println!();
        }
    }

    println!("  {}[r]{} Emergency Redemption", RED, RESET);
    println!("  {}[b]{} Back to Settings\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "r" => app.navigate_to(Screen::EmergencyRedeem),
        "b" | "B" => app.navigate_to(Screen::Settings),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_create_note(app: &mut App) -> Result<()> {
    println!("{}  CREATE NOTE{}", BOLD, RESET);
    println!("{}  ───────────{}\n", CYAN, RESET);
    println!(
        "  {}[Press Enter with empty input to cancel]{}\n",
        GRAY, RESET
    );

    if app.current_account.is_none() {
        app.set_notification("No account selected".to_string(), true);
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    let recipient = match select_pubkey_from_address_book(app, "Recipient pubkey (66 hex chars)") {
        Some(pk) => pk,
        None => {
            app.set_notification("Note creation cancelled".to_string(), false);
            app.navigate_to(Screen::Notes);
            return Ok(());
        }
    };

    let amount_str = read_input("Amount (nanoERG): ");
    if amount_str.is_empty() {
        app.set_notification("Note creation cancelled".to_string(), false);
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    if recipient.len() == 66 {
        if let Ok(amount) = amount_str.parse::<u64>() {
            // Create signing message and signature
            let issuer = app.current_account.as_ref().unwrap().pubkey.clone();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64;

            let issuer_bytes = hex::decode(&issuer)?;
            let recipient_bytes = hex::decode(&recipient)?;

            let mut key_hash_input = Vec::new();
            key_hash_input.extend_from_slice(&issuer_bytes);
            key_hash_input.extend_from_slice(&recipient_bytes);

            use blake2::{Blake2b, Digest};
            use generic_array::typenum::U32;
            let key_hash = Blake2b::<U32>::new()
                .chain_update(&key_hash_input)
                .finalize()
                .to_vec();

            let mut message = Vec::new();
            message.extend_from_slice(&key_hash);
            message.extend_from_slice(&amount.to_be_bytes());
            message.extend_from_slice(&timestamp.to_be_bytes());

            if let Some(ref acc) = app.current_account {
                if let Some(account) = app.account_manager.get_account(&acc.name) {
                    match account.sign_message(&message) {
                        Ok(signature) => {
                            let request = basis_cli_lib::api::CreateNoteRequest {
                                issuer_pubkey: issuer,
                                recipient_pubkey: recipient,
                                amount,
                                timestamp,
                                signature: hex::encode(signature),
                            };

                            match app.client.create_note(request).await {
                                Ok(_) => {
                                    app.set_notification(
                                        "Note created successfully".to_string(),
                                        false,
                                    );
                                    app.refresh_data().await?;
                                }
                                Err(e) => {
                                    app.set_notification(
                                        format!("Failed to create note: {}", e),
                                        true,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            app.set_notification(format!("Signing error: {}", e), true);
                        }
                    }
                }
            }
        } else {
            app.set_notification("Invalid amount".to_string(), true);
        }
    } else {
        app.set_notification(
            "Invalid pubkey length (must be 66 hex chars)".to_string(),
            true,
        );
    }

    app.navigate_to(Screen::Notes);
    Ok(())
}

async fn draw_redeem_note(app: &mut App) -> Result<()> {
    println!("{}  REDEEM NOTE{}", BOLD, RESET);
    println!("{}  ───────────{}\n", CYAN, RESET);

    if app.current_account.is_none() {
        app.set_notification("No account selected".to_string(), true);
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    // Refresh notes to ensure we have latest data
    if let Some(ref acc) = app.current_account {
        match app.client.get_recipient_notes(&acc.pubkey).await {
            Ok(notes) => {
                app.received_notes = notes
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
            Err(e) => {
                println!("{}  Error loading notes: {}{}", RED, e, RESET);
            }
        }
    }

    if app.received_notes.is_empty() {
        println!("{}  No notes received.{}", GRAY, RESET);
        println!(
            "  {}Tip:{} Create a note from another account first.\n",
            YELLOW, RESET
        );
        println!("  Press Enter to go back...\n");
        read_input("");
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    // Display received notes list
    println!("  {}Your Received Notes:{}", BOLD, RESET);
    for (i, note) in app.received_notes.iter().enumerate() {
        let outstanding = note.amount.saturating_sub(note.redeemed);
        println!(
            "  [{}] From: {}... | {} ERG outstanding",
            i + 1,
            &note.issuer[..16],
            outstanding as f64 / 1_000_000_000.0
        );
    }
    println!();
    println!("  {}[0]{} Cancel\n", RED, RESET);

    let selection = read_input("Select note to redeem: ");
    if selection == "0" || selection.is_empty() {
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    let idx = match selection.parse::<usize>() {
        Ok(n) if n > 0 && n <= app.received_notes.len() => n - 1,
        _ => {
            app.set_notification("Invalid selection".to_string(), true);
            app.navigate_to(Screen::Notes);
            return Ok(());
        }
    };

    let selected_note = &app.received_notes[idx];
    let issuer = selected_note.issuer.clone();
    let recipient = app.current_account.as_ref().unwrap().pubkey.clone();
    let outstanding = selected_note.amount.saturating_sub(selected_note.redeemed);

    if outstanding == 0 {
        app.set_notification("Note is fully redeemed".to_string(), true);
        app.navigate_to(Screen::Notes);
        return Ok(());
    }

    // Show selected note details
    println!("\n  {}Selected Note:{}", BOLD, RESET);
    println!("  From: {}...", &issuer[..16]);
    println!("  Amount: {} nanoERG", selected_note.amount);
    println!("  Redeemed: {} nanoERG", selected_note.redeemed);
    println!("  Outstanding: {} nanoERG", outstanding);
    println!();

    // Ask for redemption amount
    let amount_str = read_input(&format!(
        "Amount to redeem (default: {} nanoERG, Press Enter for full): ",
        outstanding
    ));

    let amount = if amount_str.is_empty() {
        outstanding
    } else {
        match amount_str.parse::<u64>() {
            Ok(a) if a <= outstanding => a,
            Ok(_) => {
                app.set_notification(
                    format!("Amount exceeds outstanding liability: {}", outstanding),
                    true,
                );
                app.navigate_to(Screen::Notes);
                return Ok(());
            }
            Err(_) => {
                app.set_notification("Invalid amount".to_string(), true);
                app.navigate_to(Screen::Notes);
                return Ok(());
            }
        }
    };

    // Fetch the full note (payment timestamp) from the server.
    let note = match app.client.get_note(&issuer, &recipient).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            app.set_notification("Note not found".to_string(), true);
            app.navigate_to(Screen::Notes);
            return Ok(());
        }
        Err(e) => {
            app.set_notification(format!("Error fetching note: {}", e), true);
            app.navigate_to(Screen::Notes);
            return Ok(());
        }
    };

    println!("\n  {}Building redemption via tracker...{}", CYAN, RESET);
    match tracker_assisted_redeem(app, &issuer, &recipient, amount, note.timestamp, false).await {
        Ok(tx_id) => {
            let short = &tx_id[..16.min(tx_id.len())];
            app.set_notification(format!("Redeemed {} nanoERG, tx {}", amount, short), false);
            let _ = app.refresh_data().await;
        }
        Err(e) => {
            app.set_notification(format!("Redemption failed: {}", e), true);
        }
    }

    app.navigate_to(Screen::Notes);
    Ok(())
}

/// Tracker-assisted redemption. The tracker builds the unsigned transaction and signs the fee
/// input(s) (`POST /redemption/build`); the TUI signs the issuer message with the local issuer
/// account, adds the reserve input's `proveDlog(recipient)` proof over the same `bytes_to_sign`,
/// and submits the fully-signed transaction (`POST /redemption/submit`).
///
/// Returns the broadcast transaction id on success.
async fn tracker_assisted_redeem(
    app: &App,
    issuer: &str,
    recipient: &str,
    amount: u64,
    timestamp: u64,
    emergency: bool,
) -> Result<String, String> {
    let issuer_pk: [u8; 33] = hex::decode(issuer)
        .map_err(|e| format!("issuer hex: {}", e))?
        .try_into()
        .map_err(|_| "issuer pubkey must be 33 bytes".to_string())?;
    let recipient_pk: [u8; 33] = hex::decode(recipient)
        .map_err(|e| format!("recipient hex: {}", e))?
        .try_into()
        .map_err(|_| "recipient pubkey must be 33 bytes".to_string())?;

    // Authoritative total debt from the tracker (must match context var #3 exactly).
    let total_debt = app
        .client
        .get_tracker_proof(issuer, recipient)
        .await
        .map_err(|e| format!("tracker proof failed: {}", e))?
        .total_debt;

    // Issuer (reserve owner) signs the redemption message.
    let message = redemption_signing_message(&issuer_pk, &recipient_pk, total_debt, timestamp);
    let issuer_account = app
        .account_manager
        .accounts
        .values()
        .find(|a| a.get_pubkey_hex() == issuer)
        .ok_or_else(|| {
            format!(
                "no local account for issuer {}... (the issuer must co-sign the redemption)",
                &issuer[..16.min(issuer.len())]
            )
        })?;
    let issuer_sig = issuer_account
        .sign_message(&message)
        .map_err(|e| format!("issuer signing failed: {}", e))?;

    // Recipient (receiver) secret for the reserve input's proveDlog(recipient).
    let cur = app.current_account.as_ref().ok_or("no current account")?;
    let receiver_account = app
        .account_manager
        .get_account(&cur.name)
        .ok_or("current account not found")?;
    let receiver_secret: [u8; 32] = hex::decode(receiver_account.get_private_key_hex())
        .map_err(|e| format!("receiver secret hex: {}", e))?
        .try_into()
        .map_err(|_| "receiver secret must be 32 bytes".to_string())?;
    let receiver_sk = SecretKey::dlog_from_bytes(&receiver_secret)
        .ok_or("invalid receiver dlog secret".to_string())?;

    // Tracker builds the unsigned tx and signs the fee input(s).
    let build = app
        .client
        .redemption_build(basis_cli_lib::api::RedemptionBuildRequest {
            issuer_pubkey: issuer.to_string(),
            recipient_pubkey: recipient.to_string(),
            amount,
            timestamp,
            issuer_signature: hex::encode(issuer_sig),
            emergency,
            tracker_box_id: None,
            change_address: None,
        })
        .await
        .map_err(|e| format!("tracker build failed: {}", e))?;

    // Reconstruct the signing material the tracker produced.
    let unsigned: UnsignedTransaction = serde_json::from_value(build.unsigned_tx)
        .map_err(|e| format!("parse unsigned tx: {}", e))?;
    let partial: Transaction =
        serde_json::from_value(build.partial_tx).map_err(|e| format!("parse partial tx: {}", e))?;
    let parse_box = |h: &str| -> Result<ErgoBox, String> {
        let bytes = hex::decode(h).map_err(|e| format!("box hex: {}", e))?;
        ErgoBox::sigma_parse_bytes(&bytes).map_err(|e| format!("box parse: {:?}", e))
    };
    let mut input_boxes = Vec::with_capacity(build.input_box_binaries.len());
    for h in &build.input_box_binaries {
        input_boxes.push(parse_box(h)?);
    }
    let mut data_boxes = Vec::with_capacity(build.data_box_binaries.len());
    for h in &build.data_box_binaries {
        data_boxes.push(parse_box(h)?);
    }
    if build.headers.len() < 10 {
        return Err(format!(
            "tracker returned {} headers (need 10)",
            build.headers.len()
        ));
    }
    let pre_header = PreHeader::from(build.headers[0].clone());
    let headers: [Header; 10] = build.headers[..10]
        .to_vec()
        .try_into()
        .map_err(|_| "headers array".to_string())?;

    // Add the reserve input (index 0) proveDlog(recipient) proof over the same bytes_to_sign.
    let signed = add_input_proof(
        &unsigned,
        Some(&partial),
        &input_boxes,
        &data_boxes,
        &pre_header,
        &headers,
        0,
        &receiver_sk,
    )
    .map_err(|e| format!("reserve proof failed: {:?}", e))?;

    let tx_json = serde_json::to_value(&signed).map_err(|e| format!("serialize tx: {}", e))?;
    app.client
        .redemption_submit(
            tx_json,
            issuer,
            recipient,
            amount,
            build.new_already_redeemed,
        )
        .await
        .map_err(|e| format!("submit failed: {}", e))
}

async fn draw_emergency_redeem(app: &mut App) -> Result<()> {
    println!("{}  EMERGENCY REDEMPTION{}", BOLD, RESET);
    println!("{}  ───────────────────{}", CYAN, RESET);
    println!();

    if app.current_account.is_none() {
        app.set_notification("No account selected".to_string(), true);
        app.navigate_to(Screen::TrackerHealth);
        return Ok(());
    }

    // Refresh notes to ensure we have latest data
    if let Some(ref acc) = app.current_account {
        match app.client.get_recipient_notes(&acc.pubkey).await {
            Ok(notes) => {
                app.received_notes = notes
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
            Err(e) => {
                println!("{}  Error loading notes: {}{}", RED, e, RESET);
            }
        }
    }

    if app.received_notes.is_empty() {
        println!("{}  No notes received.{}", GRAY, RESET);
        println!(
            "  {}Tip:{} Create a note from another account first.\n",
            YELLOW, RESET
        );
        println!("  Press Enter to go back...\n");
        read_input("");
        app.navigate_to(Screen::TrackerHealth);
        return Ok(());
    }

    // Display received notes list
    println!("  {}Your Received Notes:{}", BOLD, RESET);
    for (i, note) in app.received_notes.iter().enumerate() {
        let outstanding = note.amount.saturating_sub(note.redeemed);
        println!(
            "  [{}] From: {}... | {} ERG outstanding",
            i + 1,
            &note.issuer[..16],
            outstanding as f64 / 1_000_000_000.0
        );
    }
    println!();
    println!("  {}[0]{} Cancel\n", RED, RESET);

    let selection = read_input("Select note to redeem: ");
    if selection == "0" || selection.is_empty() {
        app.navigate_to(Screen::TrackerHealth);
        return Ok(());
    }

    let idx = match selection.parse::<usize>() {
        Ok(n) if n > 0 && n <= app.received_notes.len() => n - 1,
        _ => {
            app.set_notification("Invalid selection".to_string(), true);
            app.navigate_to(Screen::TrackerHealth);
            return Ok(());
        }
    };

    let selected_note = &app.received_notes[idx];
    let issuer = selected_note.issuer.clone();
    let recipient = app.current_account.as_ref().unwrap().pubkey.clone();
    let outstanding = selected_note.amount.saturating_sub(selected_note.redeemed);

    if outstanding == 0 {
        app.set_notification("Note is fully redeemed".to_string(), true);
        app.navigate_to(Screen::TrackerHealth);
        return Ok(());
    }

    // Show selected note details
    println!("\n  {}Selected Note:{}", BOLD, RESET);
    println!("  From: {}...", &issuer[..16]);
    println!("  Amount: {} nanoERG", selected_note.amount);
    println!("  Redeemed: {} nanoERG", selected_note.redeemed);
    println!("  Outstanding: {} nanoERG", outstanding);
    println!();

    // Ask for redemption amount
    let amount_str = read_input(&format!(
        "Amount to redeem (default: {} nanoERG, Press Enter for full): ",
        outstanding
    ));

    let amount = if amount_str.is_empty() {
        outstanding
    } else {
        match amount_str.parse::<u64>() {
            Ok(a) if a <= outstanding => a,
            Ok(_) => {
                app.set_notification(
                    format!("Amount exceeds outstanding liability: {}", outstanding),
                    true,
                );
                app.navigate_to(Screen::TrackerHealth);
                return Ok(());
            }
            Err(_) => {
                app.set_notification("Invalid amount".to_string(), true);
                app.navigate_to(Screen::TrackerHealth);
                return Ok(());
            }
        }
    };

    println!(
        "\n  {}⚠ WARNING: Emergency mode bypasses the tracker signature.{}",
        RED, RESET
    );
    println!(
        "  {}Only use this when the tracker is unresponsive or unavailable.{}\n",
        YELLOW, RESET
    );
    let confirm = read_input("Type 'yes' to confirm emergency redemption: ");
    if confirm != "yes" {
        app.set_notification("Emergency redemption cancelled".to_string(), false);
        app.navigate_to(Screen::TrackerHealth);
        return Ok(());
    }

    // Fetch the full note (payment timestamp) from the server.
    let note = match app.client.get_note(&issuer, &recipient).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            app.set_notification("Note not found".to_string(), true);
            app.navigate_to(Screen::TrackerHealth);
            return Ok(());
        }
        Err(e) => {
            app.set_notification(format!("Error fetching note: {}", e), true);
            app.navigate_to(Screen::TrackerHealth);
            return Ok(());
        }
    };

    println!(
        "\n  {}Building emergency redemption via tracker...{}",
        CYAN, RESET
    );
    match tracker_assisted_redeem(app, &issuer, &recipient, amount, note.timestamp, true).await {
        Ok(tx_id) => {
            let short = &tx_id[..16.min(tx_id.len())];
            app.set_notification(
                format!("Emergency redeemed {} nanoERG, tx {}", amount, short),
                false,
            );
            let _ = app.refresh_data().await;
        }
        Err(e) => {
            app.set_notification(format!("Emergency redemption failed: {}", e), true);
        }
    }

    app.navigate_to(Screen::TrackerHealth);
    Ok(())
}

async fn draw_create_reserve(app: &mut App) -> Result<()> {
    println!("{}  CREATE RESERVE{}", BOLD, RESET);
    println!("{}  ──────────────{}\n", CYAN, RESET);
    println!(
        "  {}[Press Enter with empty input to cancel]{}\n",
        GRAY, RESET
    );

    if app.current_account.is_none() {
        app.set_notification("No account selected".to_string(), true);
        app.navigate_to(Screen::Reserves);
        return Ok(());
    }

    let nft_id = read_input("NFT ID (64 hex chars): ");
    if nft_id.is_empty() {
        app.set_notification("Reserve creation cancelled".to_string(), false);
        app.navigate_to(Screen::Reserves);
        return Ok(());
    }

    let amount_str = read_input("Amount (nanoERG): ");
    if amount_str.is_empty() {
        app.set_notification("Reserve creation cancelled".to_string(), false);
        app.navigate_to(Screen::Reserves);
        return Ok(());
    }

    if nft_id.len() == 64 {
        if let Ok(amount) = amount_str.parse::<u64>() {
            let owner = app.current_account.as_ref().unwrap().pubkey.clone();

            let request = basis_cli_lib::api::CreateReserveRequest {
                nft_id,
                owner_pubkey: owner,
                erg_amount: amount,
            };

            match app.client.create_reserve(request).await {
                Ok(response) => {
                    println!("\n{}Reserve creation payload:{}", GREEN, RESET);
                    println!("{}Fee: {} nanoERG{}", BOLD, response.fee, RESET);
                    println!(
                        "{}Change address: {}{}",
                        BOLD, response.change_address, RESET
                    );
                    println!("\n{}Requests:{}", BOLD, RESET);
                    for (i, req) in response.requests.iter().enumerate() {
                        println!("  Request {}:", i + 1);
                        println!("    Address: {}", req.address);
                        println!("    Value: {}", req.value);
                    }
                    println!();

                    let submit = read_input("Submit to tracker node for broadcast? (y/n): ");
                    if submit == "y" || submit == "Y" {
                        match app.client.submit_reserve(response).await {
                            Ok(submission) => {
                                app.set_notification(
                                    format!(
                                        "Reserve submitted, tx {}",
                                        &submission.tx_id[..16.min(submission.tx_id.len())]
                                    ),
                                    false,
                                );
                            }
                            Err(e) => {
                                app.set_notification(
                                    format!("Failed to submit reserve: {}", e),
                                    true,
                                );
                            }
                        }
                    } else {
                        app.set_notification(
                            "Reserve payload generated (not submitted)".to_string(),
                            false,
                        );
                    }
                }
                Err(e) => {
                    app.set_notification(format!("Failed to create reserve: {}", e), true);
                }
            }
        } else {
            app.set_notification("Invalid amount".to_string(), true);
        }
    } else {
        app.set_notification(
            "Invalid NFT ID length (must be 64 hex chars)".to_string(),
            true,
        );
    }

    app.navigate_to(Screen::Reserves);
    Ok(())
}

// Address book helper
fn select_pubkey_from_address_book(app: &App, prompt_prefix: &str) -> Option<String> {
    // Collect address book contacts
    let mut all_contacts: Vec<(String, String)> = Vec::new();

    // Add accounts from account manager
    for account in app.account_manager.list_accounts() {
        let pubkey = account.get_pubkey_hex();
        if pubkey.len() == 66 {
            all_contacts.push((account.name.clone(), pubkey));
        }
    }

    // Add address book contacts (deduplicate by pubkey)
    let mut seen_pubkeys: std::collections::HashSet<String> =
        all_contacts.iter().map(|(_, pk)| pk.clone()).collect();
    for (name, pubkey) in app.address_book.iter() {
        if pubkey.len() == 66 && !seen_pubkeys.contains(pubkey) {
            all_contacts.push((name.clone(), pubkey.clone()));
            seen_pubkeys.insert(pubkey.clone());
        }
    }

    all_contacts.sort_by(|a, b| a.0.cmp(&b.0));

    if !all_contacts.is_empty() {
        println!(
            "\n  {}Available Contacts ({}):{}",
            BOLD,
            all_contacts.len(),
            RESET
        );
        for (i, (name, pubkey)) in all_contacts.iter().enumerate() {
            println!(
                "    [{}] {}: {}...{}",
                i + 1,
                name,
                &pubkey[..16],
                &pubkey[56..66]
            );
        }
        println!();
    }

    let input = read_input(&format!("{} (or contact name, or number): ", prompt_prefix));

    if input.is_empty() {
        return None;
    }

    // Check if input is a number (contact index)
    if let Ok(idx) = input.parse::<usize>() {
        if idx > 0 && idx <= all_contacts.len() {
            let (name, pubkey) = &all_contacts[idx - 1];
            println!(
                "  {}Using contact '{}' pubkey: {}...{}{}",
                GREEN,
                name,
                &pubkey[..16],
                &pubkey[56..66],
                RESET
            );
            return Some(pubkey.clone());
        }
    }

    // Check if it's a contact name
    if let Some(pubkey) = app.address_book.get(&input) {
        if pubkey.len() == 66 {
            println!(
                "  {}Using contact '{}' pubkey: {}...{}{}",
                GREEN,
                input,
                &pubkey[..16],
                &pubkey[56..66],
                RESET
            );
            return Some(pubkey.clone());
        }
    }

    // Check if it's an account name
    for account in app.account_manager.list_accounts() {
        if account.name == input {
            let pubkey = account.get_pubkey_hex();
            if pubkey.len() == 66 {
                println!(
                    "  {}Using account '{}' pubkey: {}...{}{}",
                    GREEN,
                    input,
                    &pubkey[..16],
                    &pubkey[56..66],
                    RESET
                );
                return Some(pubkey);
            }
        }
    }

    // Otherwise treat as raw pubkey
    Some(input)
}

// Helper functions

fn read_choice(prompt: &str) -> String {
    print!("{}{}{}", CYAN, prompt, RESET);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn read_input(prompt: &str) -> String {
    print!("{}{}{}", BOLD, prompt, RESET);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn wait_for_enter(prompt: &str) {
    print!("\n{}{}{}", GRAY, prompt, RESET);
    io::stdout().flush().unwrap();
    let mut _input = String::new();
    io::stdin().read_line(&mut _input).unwrap();
}

fn ratio_color(ratio: f64) -> &'static str {
    match ratio {
        r if r < 1.0 => RED,
        r if r < 1.5 => YELLOW,
        r if r < 2.0 => WHITE,
        _ => GREEN,
    }
}

fn ratio_status(ratio: f64) -> &'static str {
    match ratio {
        r if r < 1.0 => "UNDER-COLLATERALIZED",
        r if r < 1.5 => "LOW",
        r if r < 2.0 => "ADEQUATE",
        r if r < 3.0 => "GOOD",
        _ => "EXCELLENT",
    }
}

async fn draw_acceptance_policy(app: &mut App) -> Result<()> {
    use basis_core::acceptance::{AcceptanceConfig, PredicateConfig};
    use std::collections::HashSet;

    println!("{}  ACCEPTANCE POLICY{}", BOLD, RESET);
    println!("{}  ─────────────────{}\n", CYAN, RESET);

    // Display current policy summary
    let (collateral_pct, whitelist_count, blacklist_count) =
        get_policy_summary(&app.acceptance_config);
    println!(
        "  Current Mode: {}[{}% Collateral Required]{}",
        BOLD, collateral_pct, RESET
    );
    println!("  Whitelist: {} entries", whitelist_count);
    println!("  Blacklist: {} entries", blacklist_count);
    if app.server_connected {
        if app.policy_uploaded {
            println!("  {}Policy: {}synced with server{}", GRAY, GREEN, RESET);
        } else {
            println!(
                "  {}Policy: {}⚠ not uploaded to server{}",
                GRAY, YELLOW, RESET
            );
        }
    }
    println!();

    println!("  {}[1]{} Set Collateral Level (0-1000%)", CYAN, RESET);
    println!("  {}[2]{} Add to Whitelist (trust issuer)", CYAN, RESET);
    println!("  {}[3]{} Remove from Whitelist", CYAN, RESET);
    println!("  {}[4]{} Add to Blacklist (block issuer)", CYAN, RESET);
    println!("  {}[5]{} Remove from Blacklist", CYAN, RESET);
    println!("  {}[6]{} Reset to Default (100% Collateral)", CYAN, RESET);
    println!("  {}[7]{} View Current Policy", CYAN, RESET);
    println!("  {}[8]{} Test Policy Against Issuer", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => {
            let input = read_input("Enter collateral percentage (0-1000, default=100): ");
            let pct = if input.is_empty() {
                100
            } else {
                input.parse::<u16>().unwrap_or(100)
            };
            let ratio = (pct as f64) / 100.0;

            // Update collateral predicate
            let mut config = AcceptanceConfig::default_collateral();
            config.predicates[0] = PredicateConfig::Collateralization {
                name: "require_full_collateral".to_string(),
                min_ratio: ratio,
            };
            app.acceptance_config = config;

            // Save to disk and upload to server
            if let Err(e) = save_and_upload_policy(app).await {
                app.set_notification(
                    format!("⚠️ Policy saved locally but upload failed: {}", e),
                    true,
                );
            } else {
                app.set_notification(
                    format!("✅ Policy updated: {}% collateral required", pct),
                    false,
                );
            }
        }
        "2" => {
            println!("\n  Add issuer to whitelist:");
            println!("  {}[1]{} Select from Address Book", CYAN, RESET);
            println!("  {}[2]{} Enter pubkey manually\n", CYAN, RESET);

            let choice = read_choice("Select: ");
            let pubkey = if choice == "1" {
                select_pubkey_from_address_book(app, "Select contact")
            } else {
                let pk = read_input("Enter pubkey (66 hex chars): ");
                if pk.len() == 66 {
                    Some(pk)
                } else {
                    None
                }
            };

            if let Some(pubkey) = pubkey {
                if pubkey.len() != 66 {
                    app.set_notification(
                        format!(
                            "Invalid pubkey length: {} (must be 66 hex chars)",
                            pubkey.len()
                        ),
                        true,
                    );
                } else {
                    let debt_limit =
                        read_input("Add debt limit? (nanoERG, Press Enter for none): ");
                    let max_debt = if debt_limit.is_empty() {
                        None
                    } else {
                        debt_limit.parse::<u64>().ok()
                    };

                    // Add to whitelist
                    let mut holders = HashSet::new();
                    holders.insert(pubkey.clone());

                    app.acceptance_config =
                        create_policy(&app.acceptance_config, Some(holders), None, None, max_debt);

                    // Save to disk and upload to server
                    if let Err(e) = save_and_upload_policy(app).await {
                        app.set_notification(
                            format!("⚠️ Policy saved locally but upload failed: {}", e),
                            true,
                        );
                    } else {
                        let limit_msg = match max_debt {
                            Some(limit) => format!(
                                "✅ Added to whitelist (limit: {:.6} ERG) and uploaded",
                                limit as f64 / 1_000_000_000.0
                            ),
                            None => "✅ Added to whitelist (no limit) and uploaded".to_string(),
                        };
                        app.set_notification(limit_msg, false);
                    }
                }
            }
        }
        "3" => {
            // Remove from whitelist
            let whitelist = get_whitelist_entries_with_limit(&app.acceptance_config);
            if whitelist.is_empty() {
                app.set_notification("Whitelist is empty".to_string(), true);
            } else {
                println!("\n  Select issuer to remove:");
                for (i, (name, pubkey, max_debt)) in whitelist.iter().enumerate() {
                    let limit_text = match max_debt {
                        Some(limit) => {
                            format!(", limit: {:.6} ERG", *limit as f64 / 1_000_000_000.0)
                        }
                        None => ", no limit".to_string(),
                    };
                    if pubkey.len() >= 66 {
                        println!(
                            "  [{}] {}: {}...{}{}",
                            i + 1,
                            name,
                            &pubkey[..16],
                            &pubkey[56..66],
                            limit_text
                        );
                    } else {
                        println!(
                            "  [{}] {}: {} (invalid length){}",
                            i + 1,
                            name,
                            pubkey,
                            limit_text
                        );
                    }
                }
                let idx = read_choice("Select: ");
                if let Ok(n) = idx.parse::<usize>() {
                    if n > 0 && n <= whitelist.len() {
                        let pubkey = whitelist[n - 1].1.clone();
                        // Remove from whitelist
                        app.acceptance_config =
                            remove_from_whitelist(&app.acceptance_config, &pubkey);

                        // Save to disk and upload to server
                        if let Err(e) = save_and_upload_policy(app).await {
                            app.set_notification(
                                format!("⚠️ Policy saved locally but upload failed: {}", e),
                                true,
                            );
                        } else {
                            app.set_notification(
                                "✅ Removed from whitelist and uploaded".to_string(),
                                false,
                            );
                        }
                    }
                }
            }
        }
        "4" => {
            println!("\n  Add issuer to blacklist:");
            println!("  {}[1]{} Select from Address Book", CYAN, RESET);
            println!("  {}[2]{} Enter pubkey manually\n", CYAN, RESET);

            let choice = read_choice("Select: ");
            let pubkey = if choice == "1" {
                select_pubkey_from_address_book(app, "Select contact")
            } else {
                let pk = read_input("Enter pubkey (66 hex chars): ");
                if pk.len() == 66 {
                    Some(pk)
                } else {
                    None
                }
            };

            if let Some(pubkey) = pubkey {
                if pubkey.len() != 66 {
                    app.set_notification(
                        format!(
                            "Invalid pubkey length: {} (must be 66 hex chars)",
                            pubkey.len()
                        ),
                        true,
                    );
                } else {
                    let mut holders = HashSet::new();
                    holders.insert(pubkey);

                    app.acceptance_config =
                        create_policy(&app.acceptance_config, None, Some(holders), None, None);

                    // Save to disk and upload to server
                    if let Err(e) = save_and_upload_policy(app).await {
                        app.set_notification(
                            format!("⚠️ Policy saved locally but upload failed: {}", e),
                            true,
                        );
                    } else {
                        app.set_notification(
                            "✅ Added to blacklist and uploaded".to_string(),
                            false,
                        );
                    }
                }
            }
        }
        "5" => {
            // Remove from blacklist
            let blacklist = get_blacklist_entries(&app.acceptance_config);
            if blacklist.is_empty() {
                app.set_notification("Blacklist is empty".to_string(), true);
            } else {
                println!("\n  Select issuer to remove:");
                for (i, pubkey) in blacklist.iter().enumerate() {
                    if pubkey.len() >= 66 {
                        println!("  [{}] {}...{}", i + 1, &pubkey[..16], &pubkey[56..66]);
                    } else {
                        println!("  [{}] {} (invalid length)", i + 1, pubkey);
                    }
                }
                let idx = read_choice("Select: ");
                if let Ok(n) = idx.parse::<usize>() {
                    if n > 0 && n <= blacklist.len() {
                        let pubkey = blacklist[n - 1].clone();
                        app.acceptance_config =
                            remove_from_blacklist(&app.acceptance_config, &pubkey);

                        // Save to disk and upload to server
                        if let Err(e) = save_and_upload_policy(app).await {
                            app.set_notification(
                                format!("⚠️ Policy saved locally but upload failed: {}", e),
                                true,
                            );
                        } else {
                            app.set_notification(
                                "✅ Removed from blacklist and uploaded".to_string(),
                                false,
                            );
                        }
                    }
                }
            }
        }
        "6" => {
            app.acceptance_config = AcceptanceConfig::default_collateral();

            // Save to disk and upload to server
            if let Err(e) = save_and_upload_policy(app).await {
                app.set_notification(
                    format!("⚠️ Policy saved locally but upload failed: {}", e),
                    true,
                );
            } else {
                app.set_notification(
                    "✅ Reset to 100% Collateral Required and uploaded".to_string(),
                    false,
                );
            }
        }
        "7" => {
            // View current policy
            println!("\n  {}Current Policy:{}", BOLD, RESET);
            println!("  Default: Reject");
            println!("  Collateral: {}%", collateral_pct);

            let whitelist = get_whitelist_entries_with_limit(&app.acceptance_config);
            if !whitelist.is_empty() {
                println!("\n  Whitelist ({}):", whitelist.len());
                for (i, (name, pubkey, max_debt)) in whitelist.iter().enumerate() {
                    let limit_text = match max_debt {
                        Some(limit) => {
                            format!(", limit: {:.6} ERG", *limit as f64 / 1_000_000_000.0)
                        }
                        None => ", no limit".to_string(),
                    };
                    if pubkey.len() >= 66 {
                        println!(
                            "  [{}] {}: {}...{}{}",
                            i + 1,
                            name,
                            &pubkey[..16],
                            &pubkey[56..66],
                            limit_text
                        );
                    } else {
                        println!(
                            "  [{}] {}: {} (invalid length){}",
                            i + 1,
                            name,
                            pubkey,
                            limit_text
                        );
                    }
                }
            }

            let blacklist = get_blacklist_entries(&app.acceptance_config);
            if !blacklist.is_empty() {
                println!("\n  Blacklist ({}):", blacklist.len());
                for (i, pubkey) in blacklist.iter().enumerate() {
                    if pubkey.len() >= 66 {
                        println!("  [{}] {}...{}", i + 1, &pubkey[..16], &pubkey[56..66]);
                    } else {
                        println!("  [{}] {} (invalid length)", i + 1, pubkey);
                    }
                }
            }

            println!("\n  Policy Logic: NOT blacklisted AND (whitelisted OR collateralized)");
            wait_for_enter("\nPress Enter to continue...");
        }
        "8" => {
            // Test policy against issuer
            if app.current_account.is_none() {
                app.set_notification("No account selected".to_string(), true);
            } else {
                let input = read_input("Enter issuer pubkey (or contact name): ");
                let pubkey = if let Some(pk) = app.address_book.get(&input) {
                    pk.clone()
                } else {
                    input
                };

                if pubkey.len() == 66 {
                    let debt_input = read_input("Test total debt (nanoERG, default 0): ");
                    let total_debt = if debt_input.is_empty() {
                        0
                    } else {
                        debt_input.parse::<u64>().unwrap_or(0)
                    };

                    if app.server_connected {
                        let recipient = app.current_account.as_ref().unwrap().pubkey.clone();
                        match app
                            .client
                            .check_acceptance(&pubkey, total_debt, Some(&recipient))
                            .await
                        {
                            Ok(result) => {
                                if result.acceptable {
                                    println!("\n  {}✅ ACCEPTED{}", GREEN, RESET);
                                } else {
                                    println!("\n  {}❌ REJECTED{}", RED, RESET);
                                }
                                if let Some(reason) = result.reason {
                                    println!("  Reason: {}", reason);
                                }
                                wait_for_enter("\nPress Enter to continue...");
                            }
                            Err(e) => {
                                app.set_notification(
                                    format!("Server policy check failed: {}", e),
                                    true,
                                );
                            }
                        }
                    } else {
                        // Fallback to local whitelist/blacklist check
                        let whitelist = get_whitelist_entries(&app.acceptance_config);
                        let blacklist = get_blacklist_entries(&app.acceptance_config);

                        let is_blacklisted = blacklist.contains(&pubkey);
                        let is_whitelisted = whitelist.iter().any(|(_, pk)| pk == &pubkey);

                        if is_blacklisted {
                            println!("\n  {}❌ REJECTED{}", RED, RESET);
                            println!("  Reason: Blacklisted (blacklist takes precedence)");
                        } else if is_whitelisted {
                            println!("\n  {}✅ ACCEPTED{}", GREEN, RESET);
                            println!("  Reason: In whitelist");
                        } else {
                            println!("\n  {}❌ REJECTED{}", RED, RESET);
                            println!(
                                "  Reason: Not in whitelist (server offline; collateral/max_debt not evaluated)"
                            );
                        }
                        wait_for_enter("\nPress Enter to continue...");
                    }
                } else {
                    app.set_notification(
                        "Invalid pubkey length (must be 66 hex chars)".to_string(),
                        true,
                    );
                }
            }
        }
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

/// Save policy to disk and upload to server
async fn save_and_upload_policy(app: &mut App) -> Result<()> {
    // 1. Save to local config file
    app.tui_config_manager
        .update_acceptance(app.acceptance_config.clone())?;

    // 2. Upload to server if connected and account exists
    if app.server_connected {
        if let Some(ref account) = app.current_account {
            // Get the account's signing key
            if let Some(account_obj) = app.account_manager.get_account(&account.name) {
                // Serialize policy to JSON
                let policy_json = serde_json::to_string(&app.acceptance_config)?;

                // Sign the policy JSON with the account's private key
                let signature = account_obj.sign_message(policy_json.as_bytes())?;
                let signature_hex = hex::encode(&signature);

                // Create upload request
                let request = basis_cli_lib::api::UploadPolicyRequest {
                    recipient_pubkey: account.pubkey.clone(),
                    policy_json,
                    signature: signature_hex,
                };

                // Upload to server
                match app.client.upload_policy(request).await {
                    Ok(_) => {
                        app.policy_uploaded = true;
                        Ok(())
                    }
                    Err(e) => {
                        // Policy saved locally but upload failed
                        app.policy_uploaded = false;
                        Err(e)
                    }
                }
            } else {
                Err(anyhow::anyhow!("Account not found for signing"))
            }
        } else {
            Err(anyhow::anyhow!("No current account selected"))
        }
    } else {
        Err(anyhow::anyhow!("Server not connected"))
    }
}

// Helper functions are now in crate::acceptance_policy module
