//! Bounded runtime HTTP title fetching. Unit tests use local HTTP only.
use crate::Result;
use scraper::{Html, Selector};
use std::time::Duration;

pub const TIMEOUT: Duration = Duration::from_secs(5);
pub const BODY_LIMIT: usize = 1024 * 1024;

pub async fn fetch_title(url: &str) -> Result<String> {
    fetch_title_with_limits(url, TIMEOUT, BODY_LIMIT).await
}
pub async fn fetch_title_with_limits(url: &str, timeout: Duration, limit: usize) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(3)))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(timeout, async {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err("Title response exceeds body limit".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err("Title response exceeds body limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let html = Html::parse_document(&String::from_utf8_lossy(&bytes));
        let selector = Selector::parse("title").map_err(|e| e.to_string())?;
        Ok(html.select(&selector).flat_map(|n| n.text()).collect())
    })
    .await
    .map_err(|_| "Title request timed out".to_owned())?
}
