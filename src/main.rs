use std::{
    collections::HashSet,
    sync::{atomic::Ordering, Mutex, OnceLock},
    time::Duration,
};

use bug_tracker::{fetch_blog_posts, fetch_bug_disclosures};
use clap::{command, Parser};
use serde::{Deserialize, Serialize};
use tokio::{task::JoinSet, time::sleep};
use tracing::{error, info};
use twitter::post_tweet;

mod bug_tracker;
mod config;
mod twitter;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Disable tweeting
    #[arg(long)]
    disable_tweets: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    dotenv::dotenv().ok();

    let args = Args::parse();
    if args.disable_tweets {
        crate::twitter::ENABLE_TWEETING.store(false, Ordering::Relaxed)
    }

    config::load();

    loop {
        if let Err(e) = fetch_blog_posts().await {
            error!("error fetching blog posts: {}", e);
        }

        if let Err(e) = fetch_bug_disclosures().await {
            error!("error fetching bug disclosures: {}", e);
        }

        if let Err(e) = config::get().lock().unwrap().flush() {
            error!("error flushing config: {}", e);
        }

        info!("Done! Sleeping...");

        sleep(Duration::from_secs(1 * 60)).await;
    }
}
