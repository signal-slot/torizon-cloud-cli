//! Thin HTTP wrapper around the Torizon Platform API: handles the base URL,
//! bearer auth, query strings, rate-limit (HTTP 420) retries, and turning
//! non-2xx responses into errors.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::{Client as HttpClient, RequestBuilder};
use reqwest::Method;
use serde_json::Value;

use crate::auth;
use crate::config::Profile;

/// How many times to retry after an HTTP 420 (rate limited) response.
const MAX_RATE_LIMIT_RETRIES: u32 = 5;

pub struct ApiClient {
    http: HttpClient,
    profile: Profile,
    token: String,
}

/// A query parameter. Multi-valued params (e.g. repeated `deviceId`) are
/// expressed by adding the same key several times.
pub type Query<'a> = &'a [(&'a str, String)];

impl ApiClient {
    /// Build a client and acquire an access token up front.
    pub fn new(profile: Profile) -> Result<Self> {
        let http = HttpClient::builder()
            .user_agent(concat!("torizon-cloud-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building HTTP client")?;
        let token = auth::access_token(&http, &profile)?;
        Ok(Self {
            http,
            profile,
            token,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.profile.api_base(), path)
    }

    /// Build a fresh request. Called per attempt so retries can re-send.
    fn build(
        &self,
        method: &Method,
        path: &str,
        query: Query,
        json_body: Option<&Value>,
    ) -> RequestBuilder {
        let mut req = self
            .http
            .request(method.clone(), self.url(path))
            .bearer_auth(&self.token);
        for (k, v) in query {
            req = req.query(&[(k, v)]);
        }
        if let Some(body) = json_body {
            req = req.json(body);
        }
        req
    }

    fn send(
        &self,
        method: Method,
        path: &str,
        query: Query,
        json_body: Option<&Value>,
    ) -> Result<Value> {
        self.execute(path, || self.build(&method, path, query, json_body))
    }

    /// Send a request (rebuilt by `make` each attempt) honouring HTTP 420
    /// rate-limit responses by waiting for `Retry-After` seconds and retrying.
    fn execute<F: Fn() -> RequestBuilder>(&self, path: &str, make: F) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let resp = make()
                .send()
                .with_context(|| format!("requesting {path}"))?;
            if resp.status().as_u16() == 420 && attempt < MAX_RATE_LIMIT_RETRIES {
                let wait = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(2);
                attempt += 1;
                eprintln!("rate limited (HTTP 420); waiting {wait}s (retry {attempt}/{MAX_RATE_LIMIT_RETRIES})");
                sleep(Duration::from_secs(wait));
                continue;
            }
            return Self::parse(resp, path);
        }
    }

    fn parse(resp: reqwest::blocking::Response, path: &str) -> Result<Value> {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!(
                "{} returned HTTP {}: {}",
                path,
                status.as_u16(),
                text.trim()
            ));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).with_context(|| format!("parsing response from {path}"))
    }

    pub fn get(&self, path: &str, query: Query) -> Result<Value> {
        self.send(Method::GET, path, query, None)
    }

    pub fn delete(&self, path: &str) -> Result<Value> {
        self.send(Method::DELETE, path, &[], None)
    }

    /// DELETE carrying a JSON body (e.g. removing devices from a fleet).
    pub fn delete_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::DELETE, path, &[], Some(body))
    }

    pub fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::POST, path, &[], Some(body))
    }

    pub fn put_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::PUT, path, &[], Some(body))
    }

    pub fn patch_json(&self, path: &str, body: &Value) -> Result<Value> {
        self.send(Method::PATCH, path, &[], Some(body))
    }

    /// PATCH with no request body (e.g. cancelling an update).
    pub fn patch(&self, path: &str, query: Query) -> Result<Value> {
        self.send(Method::PATCH, path, query, None)
    }

    /// Upload a raw binary body (`application/octet-stream`) — used for package
    /// uploads, where metadata travels as query parameters.
    pub fn post_octet_stream(&self, path: &str, query: Query, body: Vec<u8>) -> Result<Value> {
        self.execute(path, || {
            self.http
                .post(self.url(path))
                .bearer_auth(&self.token)
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .query(
                    &query
                        .iter()
                        .map(|(k, v)| (*k, v.as_str()))
                        .collect::<Vec<_>>(),
                )
                .body(body.clone())
        })
    }
}
