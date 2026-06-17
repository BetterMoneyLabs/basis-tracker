use anyhow::Result;

mod app;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    let mut app = App::new().await?;
    ui::run(&mut app).await?;
    Ok(())
}
