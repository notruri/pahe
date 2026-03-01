mod app;
mod logger;
mod progress;
mod prompt;
mod utils;

use app::*;

#[tokio::main]
async fn main() {
    App::new().run().await;
}
