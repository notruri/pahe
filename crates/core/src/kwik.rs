use crate::errors::{KwikError, Result};
use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION, ORIGIN, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Client, Response, Url};
use std::sync::Arc;
use tracing::{debug, info};

/// resolved download information returned by kwik extraction.
#[derive(Debug, Clone)]
pub struct DirectLink {
    /// referer url that should be sent when requesting `direct_link`.
    pub referer: String,
    /// final redirected media url.
    pub direct_link: String,
}

pub struct KwikClient {
    client: Client,
    no_redirect_client: Client,
    base_alphabet: String,
}

impl KwikClient {
    /// creates a kwik client with shared cookie storage for get/post requests.
    pub fn new() -> Result<Self> {
        info!("initializing kwik client");
        let jar = Arc::new(Jar::default());

        let client = Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .map_err(|source| KwikError::BuildClient {
                context: "building reqwest client",
                source,
            })?;

        let no_redirect_client = Client::builder()
            .cookie_provider(jar)
            .redirect(Policy::none())
            .build()
            .map_err(|source| KwikError::BuildClient {
                context: "building no-redirect client",
                source,
            })?;

        Ok(Self {
            client,
            no_redirect_client,
            base_alphabet: "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ+/"
                .to_string(),
        })
    }

    fn decode_base(&self, input: &str, from_base: usize, to_base: usize) -> Result<i64> {
        let from_alphabet = &self.base_alphabet[..from_base];
        let to_alphabet = &self.base_alphabet[..to_base];

        let mut value: i64 = 0;
        for (idx, ch) in input.chars().rev().enumerate() {
            if let Some(pos) = from_alphabet.find(ch) {
                value += (pos as i64) * (from_base as i64).pow(idx as u32);
            }
        }

        if value == 0 {
            return Ok(to_alphabet
                .chars()
                .next()
                .unwrap_or('0')
                .to_string()
                .parse::<i64>()?);
        }

        let mut v = value;
        let mut out = String::new();
        while v > 0 {
            let i = (v % to_base as i64) as usize;
            out.insert(0, to_alphabet.chars().nth(i).unwrap_or('0'));
            v /= to_base as i64;
        }

        Ok(out.parse::<i64>()?)
    }

    fn decode_js_style(
        &self,
        encoded: &str,
        alphabet_key: &str,
        offset: i64,
        base: usize,
    ) -> Result<String> {
        let sentinel = alphabet_key
            .chars()
            .nth(base)
            .ok_or(KwikError::InvalidAlphabetBaseIndex { base })?;

        let mut output = String::new();
        let chars: Vec<char> = encoded.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let mut chunk = String::new();
            while i < chars.len() && chars[i] != sentinel {
                chunk.push(chars[i]);
                i += 1;
            }
            i += 1;

            let mut replaced = chunk;
            for (idx, c) in alphabet_key.chars().enumerate() {
                replaced = replaced.replace(c, &idx.to_string());
            }

            let code = self.decode_base(&replaced, base, 10)? - offset;
            output.push(char::from_u32(code as u32).unwrap_or('\0'));
        }

        Ok(output)
    }

    fn kwik_session_from_response(response: &Response) -> Option<String> {
        for cookie in response.cookies() {
            if cookie.name() == "kwik_session" {
                return Some(cookie.value().to_string());
            }
        }
        None
    }

    fn origin_from_url(url: &str) -> Option<String> {
        let parsed = Url::parse(url).ok()?;
        let host = parsed.host_str()?;
        let scheme = parsed.scheme();
        Some(format!("{scheme}://{host}"))
    }

    fn extract_link_and_token(&self, decoded: &str) -> Result<(String, String)> {
        debug!("extracting kwik form action and token from decoded payload");
        let form_action_re = Regex::new(r#"<form[^>]*action=[\"']([^\"']+)[\"']"#)?;
        let kwik_link_re = Regex::new(r#"\"(https?://kwik\.[^/\s\"]+/[^/\s\"]+/[^\"\s]*)\""#)?;

        // Prefer form action if present; this is what receives the POST.
        let link = form_action_re
            .captures(decoded)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .or_else(|| {
                kwik_link_re
                    .captures(decoded)
                    .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            })
            .ok_or(KwikError::MissingKwikPostLink)?;

        // Handle both quote styles and any attribute ordering.
        let token_re_1 = Regex::new(r#"name=[\"']_token[\"'][^>]*value=[\"']([^\"']+)[\"']"#)?;
        let token_re_2 = Regex::new(r#"value=[\"']([^\"']+)[\"'][^>]*name=[\"']_token[\"']"#)?;
        let token = token_re_1
            .captures(decoded)
            .or_else(|| token_re_2.captures(decoded))
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .ok_or(KwikError::MissingToken)?;

        debug!(%link, "extracted kwik post link and token");
        Ok((link, token))
    }

    async fn fetch_kwik_direct(&self, kwik_link: &str, token: &str) -> Result<String> {
        info!(%kwik_link, "posting kwik direct-link form");
        let mut req = self
            .no_redirect_client
            .post(kwik_link)
            .header(REFERER, kwik_link)
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36",
            )
            .header(ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .form(&[("_token", token)]);

        if let Some(origin) = Self::origin_from_url(kwik_link) {
            debug!(%origin, "setting kwik request origin header");
            req = req.header(ORIGIN, origin);
        }

        let resp = req.send().await.map_err(|source| KwikError::Request {
            context: format!("posting kwik direct link form {kwik_link}"),
            source,
        })?;

        if resp.status().as_u16() != 302 {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(KwikError::HttpStatus {
                context: "kwik direct-link post".to_string(),
                status,
                body,
            });
        }

        let location = resp
            .headers()
            .get(LOCATION)
            .and_then(|h| h.to_str().ok())
            .ok_or(KwikError::MissingRedirectLocation)?;

        debug!(%kwik_link, redirect_location = %location, "received direct link redirect");
        Ok(location.to_string())
    }

    async fn fetch_kwik_dlink(&self, kwik_link: &str, retries: u8) -> Result<String> {
        info!(%kwik_link, retries, "resolving kwik direct link");
        if retries == 0 {
            return Err(KwikError::RetryLimitExceeded {
                link: kwik_link.to_string(),
            });
        }

        let resp = self
            .client
            .get(kwik_link)
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36",
            )
            .send()
            .await
            .map_err(|source| KwikError::Request {
                context: format!("loading kwik page {kwik_link}"),
                source,
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());

            return Err(KwikError::HttpStatus {
                context: format!("kwik page {kwik_link}"),
                status,
                body,
            });
        }

        // Keep for debugging/compatibility checks; jar shares all cookies across GET/POST clients.
        let _kwik_session = Self::kwik_session_from_response(&resp);
        let page = resp
            .text()
            .await
            .map_err(|source| KwikError::ResponseBody {
                context: format!("reading kwik page body {kwik_link}"),
                source,
            })?
            .replace(['\n', '\r'], "");

        let packed_re = Regex::new(
            r#"\(\s*\"([^\",]*)\"\s*,\s*\d+\s*,\s*\"([^\",]*)\"\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*\d+[a-zA-Z]?\s*\)"#,
        )?;

        let caps = if let Some(c) = packed_re.captures(&page) {
            c
        } else {
            debug!(%kwik_link, retries_remaining = retries - 1, "packed payload not found; retrying");
            return Box::pin(self.fetch_kwik_dlink(kwik_link, retries - 1)).await;
        };

        let encoded = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let alphabet_key = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        let offset = caps
            .get(3)
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .ok_or(KwikError::InvalidOffset)?;
        let base = caps
            .get(4)
            .and_then(|m| m.as_str().parse::<usize>().ok())
            .ok_or(KwikError::InvalidBase)?;

        let decoded = match self.decode_js_style(encoded, alphabet_key, offset, base) {
            Ok(v) => v,
            Err(err) => {
                debug!(
                    %kwik_link,
                    retries_remaining = retries - 1,
                    error = %err,
                    "failed to decode packed payload; retrying"
                );
                return Box::pin(self.fetch_kwik_dlink(kwik_link, retries - 1)).await;
            }
        };

        let (link, token) = match self.extract_link_and_token(&decoded) {
            Ok(v) => v,
            Err(err) => {
                debug!(
                    %kwik_link,
                    retries_remaining = retries - 1,
                    error = %err,
                    "failed to extract post link/token; retrying"
                );
                return Box::pin(self.fetch_kwik_dlink(kwik_link, retries - 1)).await;
            }
        };

        self.fetch_kwik_direct(&link, &token).await
    }

    /// extracts a kwik referer and final direct link from a `pahe.win` page.
    pub async fn extract_kwik_link(&self, pahe_link: &str) -> Result<DirectLink> {
        info!(%pahe_link, "extracting kwik link from pahe page");
        let resp =
            self.client
                .get(pahe_link)
                .send()
                .await
                .map_err(|source| KwikError::Request {
                    context: format!("loading pahe link {pahe_link}"),
                    source,
                })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());

            return Err(KwikError::HttpStatus {
                context: format!("pahe link {pahe_link}"),
                status,
                body,
            });
        }

        let body = resp
            .text()
            .await
            .map_err(|source| KwikError::ResponseBody {
                context: format!("reading pahe body {pahe_link}"),
                source,
            })?
            .replace(['\n', '\r'], "");

        let kwik_direct_re = Regex::new(r#"\"(https?://kwik\.[^/\s\"]+/[^/\s\"]+/[^\"\s]*)\""#)?;
        let packed_re = Regex::new(
            r#"\(\s*\"([^\",]*)\"\s*,\s*\d+\s*,\s*\"([^\",]*)\"\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*\d+[a-zA-Z]?\s*\)"#,
        )?;

        let kwik_link = if let Some(cap) = kwik_direct_re.captures(&body) {
            debug!("found direct kwik link in pahe payload");
            cap.get(1).map(|m| m.as_str().to_string())
        } else if let Some(cap) = packed_re.captures(&body) {
            debug!("found packed kwik payload in pahe page; decoding");
            let encoded = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
            let alphabet_key = cap.get(2).map(|m| m.as_str()).unwrap_or_default();
            let offset = cap
                .get(3)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .ok_or(KwikError::InvalidOffset)?;
            let base = cap
                .get(4)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .ok_or(KwikError::InvalidBase)?;

            let decoded = self.decode_js_style(encoded, alphabet_key, offset, base)?;
            kwik_direct_re
                .captures(&decoded)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                .map(|v| v.replace("/d/", "/f/"))
        } else {
            None
        }
        .ok_or(KwikError::MissingKwikLink)?;

        let direct_link = self.fetch_kwik_dlink(&kwik_link, 5).await?;
        info!(%pahe_link, "resolved kwik direct link");
        Ok(DirectLink {
            referer: kwik_link,
            direct_link,
        })
    }
}
