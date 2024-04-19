use bytes::Buf;
use regex::Regex;
use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;
use tracing::error;

use crate::config;
use crate::twitter::post_tweet;

const BLOGS_FEED: &str = "https://googleprojectzero.blogspot.com/feeds/posts/default";
const ISSUE_BOARD_URL: &str = "https://bugs.chromium.org/prpc/monorail.Issues/ListIssues";
const UNIQUE_ISSUE_URL: &str = "https://bugs.chromium.org/p/project-zero/issues/detail?id=";

pub async fn fetch_blog_posts() -> anyhow::Result<()> {
    let feed_text = reqwest::get(BLOGS_FEED).await?.bytes().await?.to_vec();
    let feed = feed_rs::parser::parse(feed_text.as_slice())?;
    for item in &feed.entries {
        let mut config = config::get().lock().unwrap();
        if config.is_blog_posted(item.id.as_str()) {
            break;
        }

        // TODO: should account for max tweet length, but too lazy to
        // properly calculate
        if let Some(title) = item.title.as_ref().map(|c| c.content.as_str()) {
            let tweet = format!("{} {}", title, item.links.first().unwrap().href.as_str());
            post_tweet(tweet).await?;
            config.add_posted_blog(item.id.clone());
        }
    }

    Ok(())
}

async fn get_srf_token() -> anyhow::Result<Option<String>> {
    let list_page_text =
        reqwest::get("https://bugs.chromium.org/p/project-zero/issues/list?q=&can=1&mode=grid")
            .await?
            .text()
            .await?;

    let re = Regex::new("'token': '(?P<token>[^']+)',").expect("failed to compile regex");

    let caps = re.captures(&list_page_text);
    if let Some(caps) = caps {
        Ok(Some(caps["token"].to_string()))
    } else {
        Ok(None)
    }
}

pub async fn fetch_bug_disclosures() -> anyhow::Result<()> {
    let srf_token = get_srf_token().await?;
    if srf_token.is_none() {
        error!("Did not receive a valid SRF token");
        return Ok(());
    }

    let srf_token = srf_token.unwrap();

    let mut total_items = 1;
    let mut posted_items = 0;
    let client = reqwest::Client::new();
    let mut join_set = JoinSet::new();

    while posted_items <= total_items {
        let body = json!({
            "projectNames":["project-zero"],
            "query":"",
            "cannedQuery":1,
            "groupBySpec":"",
            "sortSpec":"",
            "pagination":   {
                "start": posted_items,
                "maxItems":500
            }
        });
        let res = client
            .post(ISSUE_BOARD_URL)
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .header("X-Xsrf-Token", srf_token.as_str())
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

        total_items = issues_response.total_results;

        let mut config = config::get().lock().unwrap();
        if let Some(issues) = issues_response.issues.as_ref() {
            for issue in issues {
                posted_items += 1;
                if config.is_bug_posted(issue.local_id) {
                    continue;
                }
                config.add_posted_bug(issue.local_id);
                join_set.spawn(post_tweet(format!(
                    "{} {}{}",
                    issue.summary, UNIQUE_ISSUE_URL, issue.local_id
                )));
            }
        } else {
            break;
        }
    }

    while let Some(res) = join_set.join_next().await {
        if let Err(e) = res? {
            error!("error posting bug disclosure tweet: {}", e);
        }
    }

    Ok(())
}

// The following was generated with https://transform.tools/json-to-rust-serde

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuesResponse {
    pub issues: Option<Vec<Issue>>,
    pub total_results: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub project_name: String,
    pub local_id: i64,
    pub summary: String,
    pub status_ref: StatusRef,
    pub owner_ref: OwnerRef,
    pub label_refs: Option<Vec<LabelRef>>,
    pub reporter_ref: ReporterRef,
    pub opened_timestamp: i64,
    pub closed_timestamp: Option<i64>,
    pub modified_timestamp: i64,
    // pub star_count: Option<i64>,
    // pub attachment_count: Option<i64>,
    // pub component_modified_timestamp: i64,
    // pub status_modified_timestamp: i64,
    // pub owner_modified_timestamp: i64,
    // #[serde(default)]
    // pub cc_refs: Vec<CcRef>,
    // pub merged_into_issue_ref: Option<MergedIntoIssueRef>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusRef {
    pub status: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRef {
    pub user_id: String,
    pub display_name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelRef {
    pub label: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReporterRef {
    pub user_id: String,
    pub display_name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcRef {
    pub user_id: String,
    pub display_name: String,
    pub is_derived: Option<bool>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedIntoIssueRef {
    pub project_name: String,
    pub local_id: i64,
}
