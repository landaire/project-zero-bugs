use bytes::Buf;
use regex::Regex;
use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::task::JoinSet;
use tracing::error;

use crate::config;
use crate::twitter::post_tweet;

const BLOGS_FEED: &str = "https://googleprojectzero.blogspot.com/feeds/posts/default";
const ISSUE_BOARD_URL: &str = "https://project-zero.issues.chromium.org/action/issues/list";
const UNIQUE_ISSUE_URL: &str = "https://project-zero.issues.chromium.org/issues/";

pub async fn fetch_blog_posts() -> anyhow::Result<()> {
    let feed_text = reqwest::get(BLOGS_FEED).await?.bytes().await?.to_vec();
    let feed = feed_rs::parser::parse(feed_text.as_slice())?;
    for item in &feed.entries {
        let config = config::get().lock().unwrap();
        if config.is_blog_posted(item.id.as_str()) {
            break;
        }
        drop(config);

        // TODO: should account for max tweet length, but too lazy to
        // properly calculate
        if let Some(title) = item.title.as_ref().map(|c| c.content.as_str()) {
            let link = item.links.iter().find(|link| {
                link.rel
                    .as_ref()
                    .map(|rel| rel == "alternate")
                    .unwrap_or_default()
            });
            if let Some(link) = link {
                let tweet = format!("{} {}", title, link.href.as_str());
                post_tweet(tweet).await?;

                let mut config = config::get().lock().unwrap();
                config.add_posted_blog(item.id.clone());
            }
        }
    }

    Ok(())
}

pub type IssuesResponse = Vec<(
    String,
    Value,
    Value,
    Value,
    Value,
    Value,
    (Vec<Vec<Value>>, String, i64),
)>;

pub async fn fetch_bug_disclosures() -> anyhow::Result<()> {
    let mut posted_items = 0;
    let client = reqwest::Client::new();
    let mut join_set = JoinSet::new();
    let mut requests = 0;

    const ISSUE_QUERY_COUNT: usize = 50;

    while requests < 10 {
        let body = json!(
            [null,null,null,null,null,["365"],["status:(open | new | assigned | accepted | closed | fixed | verified | duplicate | infeasible | intended_behavior | not_reproducible | obsolete)",null,ISSUE_QUERY_COUNT,format!("start_index:{}", posted_items)]]
        );
        let res = client
            .post(ISSUE_BOARD_URL)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await?;

        let response_text = res.text().await?;
        // Skip the first 4-bytes of the response that are used for XSS protection
        let issues_response: Result<IssuesResponse, _> = serde_json::from_str(&response_text[4..]);
        if issues_response.is_err() {
            error!("{}", &response_text);
        }
        let issues_response = issues_response?;

        let mut config = config::get().lock().unwrap();

        // The issues are stored in [0][last][0]
        let issues = &issues_response
            .first()
            .ok_or(anyhow::format_err!("issues_response has no 0 element"))?
            .6
             .0;

        for issue in issues {
            posted_items += 1;
            let issue_id = issue
                .get(1)
                .ok_or(anyhow::format_err!("issue has no ID field (idx 1)"))?
                .as_i64()
                .ok_or(anyhow::format_err!("issue ID is not a number"))?;
            if config.is_bug_posted(issue_id) {
                continue;
            }

            let issue_summary = issue
                .get(2)
                .ok_or(anyhow::format_err!("issue has no metadata (idx 2)"))?
                .as_array()
                .ok_or(anyhow::format_err!("issue metadata is not an array"))?
                .get(5)
                .ok_or(anyhow::format_err!(
                    "metadata does not have a title (5th element)"
                ))?
                .as_str()
                .ok_or(anyhow::format_err!("issue title is not a string"))?;

            config.add_posted_bug(issue_id);
            join_set.spawn(post_tweet(format!(
                "{} {}{}",
                issue_summary, UNIQUE_ISSUE_URL, issue_id
            )));
        }

        if issues.len() < ISSUE_QUERY_COUNT {
            break;
        }

        requests += 1;
    }

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res? {
            error!("error posting bug disclosure tweet: {}", e);
        }
    }

    Ok(())
}
