use anyhow::Result;
use std::io::{self, Write};

mod app;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new().await?;
    ui::run(&mut app).await?;
    Ok(())
}
