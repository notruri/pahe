use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION, ORIGIN, REFERER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{Client, Url};
use std::sync::Arc;
use tracing::{debug, info};

use crate::errors::{KwikError, ParserError, Result};
use crate::{parser, utils};

const CLIENT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct PaheLink {
    pub url: String,
    pub file_url: String,
}

impl PaheLink {
    pub fn new(url: impl Into<String>, file_url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            file_url: file_url.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KwikFile {
    pub embed: String,
    pub downloadable: String,
}

impl KwikFile {
    fn new(embed: impl Into<String>, downloadable: impl Into<String>) -> Self {
        Self {
            embed: embed.into(),
            downloadable: downloadable.into(),
        }
    }
}

/// resolved download information returned by kwik extraction.
#[derive(Debug, Clone)]
pub struct DirectLink {
    /// referer url that should be sent when requesting `direct_link`.
    pub referer: String,
    /// final redirected media url.
    pub direct_link: String,
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub referer: String,
    pub source: String,
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

    pub async fn resolve_pahe_link(&self, pahe_link: &str) -> Result<PaheLink> {
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

        let file_url = if let Some(cap) = kwik_direct_re.captures(&body) {
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

        // let links = self.resolve_file(&file_url, 5).await?;
        // info!(%pahe_link, "resolved kwik direct link");

        Ok(PaheLink::new(pahe_link, file_url))
    }

    async fn fetch_file_body(&self, file_url: impl AsRef<str>) -> Result<String> {
        let file_url = file_url.as_ref();
        let resp = self
            .client
            .get(file_url)
            .header(USER_AGENT, CLIENT_UA)
            .send()
            .await
            .map_err(|source| KwikError::Request {
                context: format!("get file: {file_url}"),
                source,
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());

            return Err(KwikError::HttpStatus {
                context: format!("read file: {file_url}"),
                status,
                body,
            });
        }

        let body = resp.text().await.map_err(|source| KwikError::Request {
            context: format!("read file: {file_url}"),
            source,
        })?;

        Ok(body)
    }

    /// resolves file from a `file_url` into downloadable and embed links
    pub async fn resolve_file(&self, file_url: impl AsRef<str>, retries: u8) -> Result<KwikFile> {
        let file_url = file_url.as_ref();

        debug!(%file_url, "extracting kwik links");

        let url = Url::parse(file_url).expect("invalid kwik file url"); // TODO

        // step 1: fetch the file body and extract the packed payload
        let page = self.fetch_file_body(url.as_str()).await?;
        let packed_re = Regex::new(
            r#"\(\s*\"([^\",]*)\"\s*,\s*\d+\s*,\s*\"([^\",]*)\"\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*\d+[a-zA-Z]?\s*\)"#,
        )?;

        let caps = if let Some(c) = packed_re.captures(&page) {
            c
        } else {
            debug!(%file_url, retries_remaining = retries - 1, "packed payload not found; retrying");
            return Box::pin(self.resolve_file(file_url, retries - 1)).await;
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
                    %file_url,
                    retries_remaining = retries - 1,
                    error = %err,
                    "failed to decode packed payload; retrying"
                );
                return Box::pin(self.resolve_file(file_url, retries - 1)).await;
            }
        };

        // step 2: extract the embed link from the decoded payload
        let embed_re = Regex::new(r"/e/[A-Za-z0-9]+")?;
        let embed_link = embed_re
            .captures_iter(&decoded)
            .next()
            .and_then(|m| {
                m.get(0).map(|m| {
                    format!(
                        "https://{}{}",
                        url.host().expect("kwik file url must have a host"),
                        m.as_str()
                    )
                })
            })
            .ok_or(KwikError::InvalidEmbedLink)?;
        let embed_link = Url::parse(&embed_link).expect("invalid kwik embed link");

        debug!(%embed_link, "resolved kwik embed link");

        // step 3: extract the link and token from the decoded payload
        //         and resolve it into a direct download link
        let (link, token) = self.extract_link_and_token(&decoded)?;
        let download_link = self.fetch_kwik_direct(&link, &token).await?;

        debug!(%download_link, "resolved kwik download link");

        Ok(KwikFile::new(embed_link, download_link))
    }

    pub async fn extract_kwik_stream(&self, embed_link: impl AsRef<str>) -> Result<Stream> {
        let embed_link = embed_link.as_ref();

        // step 1: extract embed body
        info!(%embed_link, "extracting embed");

        let resp =
            self.client
                .get(embed_link)
                .header(
                    USER_AGENT,
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36",
                )
                .send()
                .await
                .map_err(|source| KwikError::Request {
                    context: format!("loading embed {embed_link}"),
                    source,
                })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());

            return Err(KwikError::HttpStatus {
                context: format!("embed {embed_link}"),
                status,
                body,
            });
        }

        let body = resp
            .text()
            .await
            .map_err(|source| KwikError::ResponseBody {
                context: format!("reading embed body {embed_link}"),
                source,
            })?;

        // step 2: extract packed payload
        let packed_payload = self.extract_embed_packed(&body)?;
        let packed = self.decode_embed_payload(&packed_payload)?;

        // step 3: unpack payload
        let unpacked = utils::unpack_de(
            packed.payload.clone(),
            packed.radix as u32,
            packed.count,
            packed.symbols.clone().unwrap(),
        );

        // step 4: parse variables and extract stream url
        let variables = parser::parse_variables(unpacked)?;
        let stream_url = variables
            .iter()
            .find(|v| v.ident == "source")
            .map(|v| v.value.clone())
            .ok_or_else(|| KwikError::NoStreamURL)?;

        let stream = Stream {
            referer: embed_link.into(),
            source: stream_url,
        };

        Ok(stream)
    }

    pub fn extract_embed_packed(&self, body: impl AsRef<str>) -> Result<String> {
        let re =
            Regex::new(r#"(?s)<script\b[^>]*>(eval.*?)</script>"#).expect("compilation failed");
        let cap = re.captures(body.as_ref());
        let payload = cap
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string());

        debug!("extracted embed payload");
        let result = payload.ok_or(ParserError::ExtractError {
            context: "extract embed".into(),
        })?;

        Ok(result)
    }

    pub fn decode_embed_payload(&self, payload: impl AsRef<str>) -> Result<parser::PackedCall> {
        let result =
            parser::parse_embed_payload(payload).map_err(|_| ParserError::DecodeError {
                context: "failed to parse embed payload".into(),
            })?;
        let result = result
            .get(1)
            .ok_or(ParserError::DecodeError {
                context: "decode embed".into(),
            })
            .cloned()?;

        Ok(result)
    }
}
