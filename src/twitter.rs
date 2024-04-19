use std::sync::atomic::AtomicBool;

use tracing::info;
use twitter_v2::{
    authorization::{BearerToken, Oauth1aToken, Oauth2Token},
    TwitterApi,
};

pub static ENABLE_TWEETING: AtomicBool = AtomicBool::new(true);

pub async fn post_tweet(text: String) -> anyhow::Result<()> {
    let enable_tweeting = ENABLE_TWEETING.load(std::sync::atomic::Ordering::Relaxed);
    if !enable_tweeting {
        info!("Would be posting tweet: {}", text);
    } else {
        info!("Posting tweet: {}", text);
    }
    if enable_tweeting {
        let auth = Oauth1aToken::new(
            std::env::var("CONSUMER_KEY").unwrap(),
            std::env::var("CONSUMER_SECRET").unwrap(),
            std::env::var("ACCESS_TOKEN").unwrap(),
            std::env::var("ACCESS_TOKEN_SECRET").unwrap(),
        );

        let api = TwitterApi::new(auth);
        info!(
            "attempting to get user info {:?}",
            api.get_users_by_usernames(["landaire"]).send().await
        );

        api.post_tweet().text(text).send().await?;
    }

    Ok(())
}
