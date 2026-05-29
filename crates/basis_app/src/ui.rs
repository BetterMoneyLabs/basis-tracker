use crate::app::{App, NoteInfo, ReserveInfo, Screen};
use anyhow::Result;
use std::io::{self, Write};

// ANSI Color codes
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const CYAN: &str = "\x1b[36m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const MAGENTA: &str = "\x1b[35m";
pub const WHITE: &str = "\x1b[37m";
pub const GRAY: &str = "\x1b[90m";

pub async fn run(app: &mut App) -> Result<()> {
    clear_screen();
    print_banner();
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
            Screen::Transactions => draw_transactions(app).await?,
            Screen::Settings => draw_settings(app).await?,
            Screen::CreateNote => draw_create_note(app).await?,
            Screen::RedeemNote => draw_redeem_note(app).await?,
            Screen::CreateReserve => draw_create_reserve(app).await?,
            Screen::GenerateTransaction => draw_generate_transaction(app).await?,
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
    println!(
        "{}██████╗  █████╗ ███████╗██╗███████╗{}",
        CYAN, RESET
    );
    println!(
        "{}██╔══██╗██╔══██╗██╔════╝██║██╔════╝{}",
        CYAN, RESET
    );
    println!(
        "{}██████╔╝███████║███████╗██║███████╗{}",
        CYAN, RESET
    );
    println!(
        "{}██╔══██╗██╔══██║╚════██║██║╚════██║{}",
        CYAN, RESET
    );
    println!(
        "{}██████╔╝██║  ██║███████║██║███████║{}",
        CYAN, RESET
    );
    println!(
        "{}╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝╚══════╝{}",
        CYAN, RESET
    );
    println!();
    println!(
        "{}        Wallet v0.1.0{}",
        GRAY, RESET
    );
    println!();
    println!(
        "{}  Free Banking For Everyone{}",
        RED, RESET
    );
    println!(
        "{}  Interactive Terminal Wallet for Basis Tracker{}",
        GRAY, RESET
    );
    println!();
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
        println!(
            "{} {} {}{}{}\n",
            color, icon, BOLD, msg, RESET
        );
    }
}

async fn draw_main_menu(app: &mut App) -> Result<()> {
    println!("{}  MAIN MENU{}", BOLD, RESET);
    println!(
        "{}  ─────────{}\n",
        CYAN, RESET
    );

    println!("  {}[1]{} Accounts Management", CYAN, RESET);
    println!("  {}[2]{} Notes (IOU Debt)", CYAN, RESET);
    println!("  {}[3]{} Reserves & Collateral", CYAN, RESET);
    println!("  {}[4]{} Transactions & Redemptions", CYAN, RESET);
    println!("  {}[5]{} Address Book", CYAN, RESET);
    println!("  {}[6]{} Settings", CYAN, RESET);
    println!();
    println!("  {}[r]{} Refresh Data", YELLOW, RESET);
    println!("  {}[q]{} Quit\n", RED, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => app.navigate_to(Screen::Accounts),
        "2" => app.navigate_to(Screen::Notes),
        "3" => app.navigate_to(Screen::Reserves),
        "4" => app.navigate_to(Screen::Transactions),
        "5" => app.navigate_to(Screen::AddressBook),
        "6" => app.navigate_to(Screen::Settings),
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
    println!(
        "{}  ─────────{}\n",
        CYAN, RESET
    );

    let accounts: Vec<_> = app.account_manager.list_accounts().into_iter().map(|a| a.clone()).collect();

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
                    GREEN, i + 1, account.name, CYAN, RESET
                );
            } else {
                println!("    [{}] {}", i + 1, account.name);
            }

            println!(
                "      {}Pubkey: {}...{}{}",
                GRAY, &account.get_pubkey_hex()[..16], &account.get_pubkey_hex()[50..56], RESET
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
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "c" => {
            let name = read_input("Enter account name: ");
            if !name.is_empty() {
                match app.account_manager.create_account(&name) {
                    Ok(account) => {
                        app.set_notification(
                            format!("Created account '{}'", account.name),
                            false,
                        );
                        app.current_account = Some(crate::app::AccountInfo {
                            name: account.name.clone(),
                            pubkey: account.get_pubkey_hex(),
                            created_at: account.created_at,
                        });
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
                                    created_at: accounts[idx - 1].created_at,
                                });
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
                match basis_cli_lib::account::Account::from_private_key_hex(&name, &key,
                ) {
                    Ok(account) => {
                        let pubkey = account.get_pubkey_hex();
                        app.account_manager
                            .config_manager
                            .add_account(&name, &pubkey, &key)?;
                        app.set_notification(
                            format!("Imported account '{}'", name),
                            false,
                        );
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
                            app.set_notification(
                                "Account deletion not yet implemented".to_string(),
                                true,
                            );
                        }
                    }
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

async fn draw_address_book(app: &mut App) -> Result<()> {
    println!("{}  ADDRESS BOOK{}", BOLD, RESET);
    println!(
        "{}  ─────────────{}\n",
        CYAN, RESET
    );

    if app.address_book.is_empty() {
        println!("{}  No contacts found.{}\n", GRAY, RESET);
    } else {
        println!("  {}Contacts:{}", BOLD, RESET);
        let mut contacts: Vec<_> = app.address_book.iter().collect();
        contacts.sort_by(|a, b| a.0.cmp(b.0));
        for (i, (name, pubkey)) in contacts.iter().enumerate() {
            println!(
                "  [{}] {}: {}...{}",
                i + 1,
                name,
                &pubkey[..16],
                &pubkey[56..66]
            );
        }
        println!();
    }

    println!("  {}[a]{} Add Contact", CYAN, RESET);
    println!("  {}[d]{} Delete Contact", RED, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "a" => {
            let name = read_input("Contact name: ");
            if !name.is_empty() {
                let pubkey = read_input("Public key (66 hex chars): ");
                if pubkey.len() == 66 {
                    app.address_book.insert(name.clone(), pubkey);
                    app.set_notification(
                        format!("Added contact '{}'", name),
                        false,
                    );
                } else {
                    app.set_notification(
                        "Invalid pubkey length (must be 66 hex chars)".to_string(),
                        true,
                    );
                }
            }
        }
        "d" => {
            if !app.address_book.is_empty() {
                let name = read_input("Contact name to delete: ");
                if app.address_book.remove(&name).is_some() {
                    app.set_notification(
                        format!("Deleted contact '{}'", name),
                        false,
                    );
                } else {
                    app.set_notification(
                        format!("Contact '{}' not found", name),
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

async fn draw_notes(app: &mut App) -> Result<()> {
    println!("{}  NOTES (IOU Debt){}", BOLD, RESET);
    println!(
        "{}  ─────────────────{}\n",
        CYAN, RESET
    );

    println!("  {}[1]{} Notes Issued ({})", CYAN, RESET, app.issued_notes.len());
    println!(
        "  {}[2]{} Notes Received ({})\n",
        CYAN, RESET, app.received_notes.len()
    );

    println!("  {}[c]{} Create Note", CYAN, RESET);
    println!("  {}[r]{} Redeem Note", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => {
            println!("\n{}  Notes Issued:{}", BOLD, RESET);
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
            println!("\n{}  Notes Received:{}", BOLD, RESET);
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
    println!("{}  RESERVES & COLLATERAL{}", BOLD, RESET);
    println!(
        "{}  ───────────────────────{}\n",
        CYAN, RESET
    );

    if let Some(ref reserve) = app.reserve_status {
        let ratio_color = ratio_color(reserve.ratio);
        let status = ratio_status(reserve.ratio);

        println!("  {}Issuer:{}", BOLD, RESET);
        println!("  {}...{}\n", &reserve.issuer[..20], &reserve.issuer[46..56]);

        println!(
            "  {}Total Debt:{}     {} nanoERG ({:.6} ERG)",
            BOLD, RESET, reserve.total_debt, reserve.total_debt as f64 / 1_000_000_000.0
        );
        println!(
            "  {}Collateral:{}     {} nanoERG ({:.6} ERG)",
            BOLD, RESET, reserve.collateral, reserve.collateral as f64 / 1_000_000_000.0
        );
        println!(
            "  {}Ratio:{}          {}{}{}",
            BOLD, RESET, ratio_color, reserve.ratio, RESET
        );
        println!(
            "  {}Status:{}         {}{}{}",
            BOLD, RESET, ratio_color, status, RESET
        );
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
        println!(
            "  {}No reserve data available.{}\n",
            GRAY, RESET
        );
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

async fn draw_transactions(app: &mut App) -> Result<()> {
    println!("{}  TRANSACTIONS & REDEMPTIONS{}", BOLD, RESET);
    println!(
        "{}  ───────────────────────────{}\n",
        CYAN, RESET
    );

    println!("  {}[1]{} Generate Redemption Transaction", CYAN, RESET);
    println!();
    println!("  {}[b]{} Back to Menu\n", YELLOW, RESET);

    match read_choice("Select option: ").as_str() {
        "1" => app.navigate_to(Screen::GenerateTransaction),
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_settings(app: &mut App) -> Result<()> {
    println!("{}  SETTINGS{}", BOLD, RESET);
    println!(
        "{}  ─────────{}\n",
        CYAN, RESET
    );

    println!("  {}Tracker URL:{} {}", BOLD, RESET, app.server_url);
    println!();

    println!("  {}[1]{} Change Tracker URL", CYAN, RESET);
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
                app.set_notification(
                    format!("Tracker URL updated to: {}", new_url),
                    false,
                );
            }
        }
        "b" | "B" => app.navigate_to(Screen::MainMenu),
        _ => {
            app.set_notification("Invalid option".to_string(), true);
        }
    }

    Ok(())
}

async fn draw_create_note(app: &mut App) -> Result<()> {
    println!("{}  CREATE NOTE{}", BOLD, RESET);
    println!(
        "{}  ───────────{}\n",
        CYAN, RESET
    );
    println!("  {}[Press Enter with empty input to cancel]{}\n", GRAY, RESET);

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
                            app.set_notification(
                                format!("Signing error: {}", e),
                                true,
                            );
                        }
                    }
                }
            }
        } else {
            app.set_notification("Invalid amount".to_string(), true);
        }
    } else {
        app.set_notification("Invalid pubkey length (must be 66 hex chars)".to_string(), true);
    }

    app.navigate_to(Screen::Notes);
    Ok(())
}

async fn draw_redeem_note(app: &mut App) -> Result<()> {
    println!("{}  REDEEM NOTE{}", BOLD, RESET);
    println!(
        "{}  ───────────{}\n",
        CYAN, RESET
    );

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
                        timestamp: n.timestamp,
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
        println!("  {}Tip:{} Create a note from another account first.\n", YELLOW, RESET);
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
                    format!("Amount exceeds outstanding debt: {}", outstanding),
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

    // Get full note details from server
    match app.client.get_note(&issuer, &recipient).await {
        Ok(Some(note)) => {
            let timestamp = note.timestamp;

            // Create signing message
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
            message.extend_from_slice(&note.amount_collected.to_be_bytes());
            message.extend_from_slice(&timestamp.to_be_bytes());

            if let Some(ref acc) = app.current_account {
                if let Some(account) = app.account_manager.get_account(&acc.name) {
                    match account.sign_message(&message) {
                        Ok(signature) => {
                            let request = basis_cli_lib::api::RedeemRequest {
                                issuer_pubkey: issuer.clone(),
                                recipient_pubkey: recipient.clone(),
                                amount,
                                timestamp,
                                reserve_box_id: String::new(),
                                tracker_box_id: String::new(),
                                tracker_nft_id: String::new(),
                                current_height: 0,
                                recipient_address: String::new(),
                                change_address: String::new(),
                                issuer_signature: hex::encode(&signature),
                                emergency: false,
                                tracker_signature: None,
                            };

                            match app.client.initiate_redemption(request).await {
                                Ok(response) => {
                                    let complete_request = basis_cli_lib::api::CompleteRedemptionRequest {
                                        issuer_pubkey: issuer,
                                        recipient_pubkey: recipient,
                                        redeemed_amount: amount,
                                    };

                                    match app.client.complete_redemption(complete_request).await {
                                        Ok(_) => {
                                            app.set_notification(
                                                format!("Redeemed {} nanoERG", amount),
                                                false,
                                            );
                                            app.refresh_data().await?;
                                        }
                                        Err(e) => {
                                            app.set_notification(
                                                format!("Failed to complete redemption: {}", e),
                                                true,
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    app.set_notification(
                                        format!("Failed to initiate redemption: {}", e),
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
        }
        Ok(None) => {
            app.set_notification("Note not found".to_string(), true);
        }
        Err(e) => {
            app.set_notification(format!("Error: {}", e), true);
        }
    }

    app.navigate_to(Screen::Notes);
    Ok(())
}

async fn draw_create_reserve(app: &mut App) -> Result<()> {
    println!("{}  CREATE RESERVE{}", BOLD, RESET);
    println!(
        "{}  ──────────────{}\n",
        CYAN, RESET
    );
    println!("  {}[Press Enter with empty input to cancel]{}\n", GRAY, RESET);

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
                    println!("{}Change address: {}{}", BOLD, response.change_address, RESET);
                    println!("\n{}Requests:{}", BOLD, RESET);
                    for (i, req) in response.requests.iter().enumerate() {
                        println!("  Request {}:", i + 1);
                        println!("    Address: {}", req.address);
                        println!("    Value: {}", req.value);
                    }
                    println!();
                    wait_for_enter("Press Enter to continue...");
                    app.set_notification("Reserve payload generated".to_string(), false);
                }
                Err(e) => {
                    app.set_notification(
                        format!("Failed to create reserve: {}", e),
                        true,
                    );
                }
            }
        } else {
            app.set_notification("Invalid amount".to_string(), true);
        }
    } else {
        app.set_notification("Invalid NFT ID length (must be 64 hex chars)".to_string(), true);
    }

    app.navigate_to(Screen::Reserves);
    Ok(())
}

async fn draw_generate_transaction(app: &mut App) -> Result<()> {
    println!("{}  GENERATE REDEMPTION TRANSACTION{}", BOLD, RESET);
    println!(
        "{}  ───────────────────────────────{}\n",
        CYAN, RESET
    );
    println!("  {}[Press Enter with empty input to cancel]{}\n", GRAY, RESET);

    if app.current_account.is_none() {
        app.set_notification("No account selected".to_string(), true);
        app.navigate_to(Screen::Transactions);
        return Ok(());
    }

    let issuer = match select_pubkey_from_address_book(app, "Issuer pubkey (66 hex chars)") {
        Some(pk) => pk,
        None => {
            app.set_notification("Transaction generation cancelled".to_string(), false);
            app.navigate_to(Screen::Transactions);
            return Ok(());
        }
    };

    let recipient = match select_pubkey_from_address_book(app, "Recipient pubkey (66 hex chars)") {
        Some(pk) => pk,
        None => {
            app.set_notification("Transaction generation cancelled".to_string(), false);
            app.navigate_to(Screen::Transactions);
            return Ok(());
        }
    };

    let amount_str = read_input("Amount (nanoERG): ");
    if amount_str.is_empty() {
        app.set_notification("Transaction generation cancelled".to_string(), false);
        app.navigate_to(Screen::Transactions);
        return Ok(());
    }

    let emergency_str = read_input("Emergency redemption? (y/n): ");
    let emergency = emergency_str == "y" || emergency_str == "Y";

    if issuer.len() == 66 && recipient.len() == 66 {
        if let Ok(amount) = amount_str.parse::<u64>() {
            // Get note info
            match app.client.get_note(&issuer, &recipient).await {
                Ok(Some(note)) => {
                    if note.outstanding_debt() < amount {
                        app.set_notification(
                            "Insufficient outstanding debt".to_string(),
                            true,
                        );
                        app.navigate_to(Screen::Transactions);
                        return Ok(());
                    }

                    // Get reserve box
                    match app.client.get_reserves_by_issuer(&issuer).await {
                        Ok(reserves) => {
                            if let Some(reserve) = reserves.first() {
                                let reserve_box_id = reserve.box_id.clone();
                                let tracker_nft_id =
                                    reserve.base_info.tracker_nft_id.clone();

                                // Get tracker box
                                match app.client.get_latest_tracker_box_id().await {
                                    Ok(tracker_box) => {
                                        let tracker_box_id = tracker_box.tracker_box_id;

                                        // Get proofs
                                        match app.client.get_tracker_proof(&issuer, &recipient,
                                        ).await
                                        {
                                            Ok(tracker_proof) => {
                                                let total_debt = tracker_proof.total_debt;
                                                let tracker_lookup_proof = tracker_proof.proof;
                                                let tracker_state_digest =
                                                    tracker_proof.tracker_state_digest;

                                                // Get issuer signature
                                                let issuer_bytes = hex::decode(&issuer,
                                                )?;
                                                let recipient_bytes = hex::decode(&recipient,
                                                )?;

                                                let mut key_hash_input = Vec::new();
                                                key_hash_input.extend_from_slice(
                                                    &issuer_bytes,
                                                );
                                                key_hash_input.extend_from_slice(
                                                    &recipient_bytes,
                                                );

                                                use blake2::{Blake2b, Digest};
                                                use generic_array::typenum::U32;
                                                let key_hash = Blake2b::<U32>::new()
                                                    .chain_update(&key_hash_input)
                                                    .finalize()
                                                    .to_vec();

                                                let mut message = Vec::with_capacity(48);
                                                message.extend_from_slice(
                                                    &key_hash,
                                                );
                                                message.extend_from_slice(
                                                    &total_debt.to_be_bytes(),
                                                );
                                                message.extend_from_slice(
                                                    &note.timestamp.to_be_bytes(),
                                                );

                                                if let Some(ref acc) = app.current_account {
                                                    if let Some(account) = app.account_manager.get_account(
                                                        &acc.name,
                                                    ) {
                                                        match account.sign_message(
                                                            &message,
                                                        ) {
                                                            Ok(issuer_signature) => {
                                                                // Get tracker signature
                                                                match app.client.request_tracker_signature(
                                                                    &issuer,
                                                                    &recipient,
                                                                    total_debt,
                                                                    note.timestamp,
                                                                    emergency,
                                                                ).await
                                                                {
                                                                    Ok(tracker_sig_response) => {
                                                                        println!("\n{}Transaction generated successfully!{}", GREEN, RESET);
                                                                        println!("\n{}Details:{}", BOLD, RESET);
                                                                        println!("  Issuer: {}", issuer);
                                                                        println!("  Recipient: {}", recipient);
                                                                        println!("  Amount: {} nanoERG", amount);
                                                                        println!("  Total Debt: {} nanoERG", total_debt);
                                                                        println!("  Reserve Box: {}", reserve_box_id);
                                                                        println!("  Tracker Box: {}", tracker_box_id);
                                                                        println!("  Emergency: {}", emergency);
                                                                        println!("\n{}Signature:{}", BOLD, RESET);
                                                                        println!("  Issuer: {}...", hex::encode(&issuer_signature)[..32].to_string());
                                                                        println!("  Tracker: {}...", tracker_sig_response.tracker_signature[..32].to_string());
                                                                        println!();
                                                                        wait_for_enter("Press Enter to continue...");
                                                                        app.set_notification("Transaction generated".to_string(), false);
                                                                    }
                                                                    Err(e) => {
                                                                        app.set_notification(format!("Tracker signature error: {}", e), true);
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                app.set_notification(format!("Signing error: {}", e), true);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                app.set_notification(format!("Tracker proof error: {}", e), true);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        app.set_notification(format!("Tracker box error: {}", e), true);
                                    }
                                }
                            } else {
                                app.set_notification("No reserve found".to_string(), true);
                            }
                        }
                        Err(e) => {
                            app.set_notification(format!("Reserve error: {}", e), true);
                        }
                    }
                }
                Ok(None) => {
                    app.set_notification("Note not found".to_string(), true);
                }
                Err(e) => {
                    app.set_notification(format!("Note error: {}", e), true);
                }
            }
        } else {
            app.set_notification("Invalid amount".to_string(), true);
        }
    } else {
        app.set_notification("Invalid pubkey length (must be 66 hex chars)".to_string(), true);
    }

    app.navigate_to(Screen::Transactions);
    Ok(())
}

// Address book helper
fn select_pubkey_from_address_book(app: &App, prompt_prefix: &str) -> Option<String> {
    if !app.address_book.is_empty() {
        println!("\n  {}Address Book Contacts:{}", BOLD, RESET);
        let mut contacts: Vec<_> = app.address_book.iter().collect();
        contacts.sort_by(|a, b| a.0.cmp(b.0));
        for (i, (name, pubkey)) in contacts.iter().enumerate() {
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

    let input = read_input(&format!("{} (or contact name): ", prompt_prefix));

    if input.is_empty() {
        return None;
    }

    // Check if it's a contact name
    if let Some(pubkey) = app.address_book.get(&input) {
        println!("  {}Using contact '{}' pubkey: {}...{}{}", GREEN, input, &pubkey[..16], &pubkey[56..66], RESET);
        return Some(pubkey.clone());
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
