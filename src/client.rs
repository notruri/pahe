use regex::Regex;
use reqwest::cookie::Jar;
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, COOKIE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT,
};
use reqwest::{Client as ReqwestClient, Url};
use scraper::{Html, Selector};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, info};

use pahe_core::{DirectLink, KwikClient};

use crate::errors::{PaheError, Result};

#[derive(Debug, Clone)]
pub struct Anime {
    pub id: String,
    pub title: Option<String>,
}

/// download variant metadata parsed from a single animepahe play page.
#[derive(Debug, Clone)]
pub struct EpisodeVariant {
    /// mirror link hosted on `pahe.win` that can be resolved into a direct file url.
    pub dpahe_link: String,
    /// raw text block extracted from the source anchor html.
    pub source_text: String,
    /// declared video resolution (for example `720` or `1080`).
    pub resolution: i32,
    /// normalized audio language (`jp`, `eng`, `zh`, or fallback value).
    pub lang: String,
    /// bluray encoded.
    pub bluray: bool,
}

/// selection result that pairs a play page with the chosen variant.
#[derive(Debug, Clone)]
pub struct EpisodeSelection {
    /// play page url used to build this selection.
    pub play_link: String,
    /// chosen variant for the play page.
    pub variant: EpisodeVariant,
}

#[derive(Debug, Deserialize)]
struct ReleasePage {
    total: i32,
    data: Vec<ReleaseItem>,
}

#[derive(Debug, Deserialize)]
struct ReleaseItem {
    episode: u32,
    session: String,
}

pub struct PaheClient {
    base_domain: String,
    redirect_domain: String,
    client: ReqwestClient,
    kwik: KwikClient,
    cookie_header: Option<String>,
}

impl PaheClient {
    /// creates a client without an explicit clearance cookie header.
    ///
    /// this is enough when animepahe is accessible without triggering ddos-guard.
    pub fn new(base_domain: String, redirect_domain: String) -> Result<Self> {
        Self::with_cookie_header(base_domain, redirect_domain, None)
    }

    /// creates a client with a browser-exported cookie header.
    ///
    /// use this when animepahe returns ddos-guard challenge pages.
    pub fn new_with_clearance_cookie(
        base_domain: String,
        redirect_domain: String,
        cookie_header: impl Into<String>,
    ) -> Result<Self> {
        Self::with_cookie_header(base_domain, redirect_domain, Some(cookie_header.into()))
    }

    fn with_cookie_header(
        base_domain: String,
        redirect_domain: String,
        cookie_header: Option<String>,
    ) -> Result<Self> {
        info!(
            %base_domain,
            %redirect_domain,
            has_cookie_header = cookie_header.is_some(),
            "initializing pahe client"
        );
        let jar = Arc::new(Jar::default());
        let animepahe_base = Url::parse(format!("https://{base_domain}/").as_ref())
            .map_err(|_| PaheError::AnimepaheBaseUrl)?;

        if let Some(ref cookie) = cookie_header {
            let mut loaded_cookies = 0usize;
            for part in cookie.split(';') {
                let piece = part.trim();
                if !piece.is_empty() && piece.contains('=') {
                    jar.add_cookie_str(piece, &animepahe_base);
                    loaded_cookies += 1;
                }
            }
            debug!(loaded_cookies, "loaded cookies into reqwest cookie jar");
        }

        let client = ReqwestClient::builder()
            .cookie_provider(jar)
            .build()
            .map_err(PaheError::BuildClient)?;

        Ok(Self {
            base_domain,
            redirect_domain,
            client,
            kwik: KwikClient::new()?,
            cookie_header,
        })
    }

    fn headers(&self, referer: &str, is_api: bool) -> HeaderMap {
        debug!(%referer, is_api, "building request headers");
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(if is_api {
                "application/json, text/javascript, */*; q=0.0"
            } else {
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
            }),
        );
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"));

        if let Ok(v) = HeaderValue::from_str(referer) {
            headers.insert(REFERER, v);
        }

        if let Ok(v) = HeaderValue::from_str(format!("https://{}/", self.base_domain).as_ref()) {
            headers.insert(ORIGIN, v);
        }

        if let Some(cookie) = &self.cookie_header
            && let Ok(v) = HeaderValue::from_str(cookie)
        {
            headers.insert(COOKIE, v);
        }

        headers
    }

    fn anime_id(link: &str) -> Result<String> {
        debug!(%link, "extracting anime id from link");
        let re = Regex::new(r"anime/([a-f0-9-]{36})")?;
        let id = re
            .captures(link)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            .ok_or_else(|| PaheError::InvalidAnimeLink {
                link: link.to_string(),
            })?;
        debug!(anime_id = %id, "anime id extracted");
        Ok(id)
    }

    fn detect_ddos_guard(body: &str) -> bool {
        body.contains("DDoS-Guard")
            || body.contains("/.well-known/ddos-guard/js-challenge")
            || body.contains("Checking your browser before accessing")
    }

    async fn ensure_success_or_ddg(
        response: reqwest::Response,
        context: &str,
        cookie_hint: bool,
    ) -> Result<reqwest::Response> {
        if response.status().is_success() {
            debug!(%context, "received successful response");
            return Ok(response);
        }

        let status = response.status();
        info!(%context, %status, "request returned non-success status");
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<failed to read error body>".to_string());

        if status.as_u16() == 403 && Self::detect_ddos_guard(&body) {
            info!(%context, "ddos-guard challenge detected");
            let hint = if cookie_hint {
                "DDoS-Guard challenge detected even with provided cookie header. Refresh cookies from a real browser session."
            } else {
                "DDoS-Guard challenge detected. Solve challenge in a real browser and initialize .cookies_str(COOKIES)"
            };
            return Err(PaheError::DdosGuard {
                context: context.to_string(),
                hint: hint.to_string(),
            });
        }

        Err(PaheError::HttpStatus {
            context: context.to_string(),
            status,
            body,
        })
    }

    pub async fn get_series_metadata(&self, series_link: &str) -> Result<Anime> {
        info!(%series_link, "fetching series metadata");
        let id = Self::anime_id(series_link)?;

        let resp = self
            .client
            .get(series_link)
            .headers(self.headers(series_link, false))
            .send()
            .await
            .map_err(|source| PaheError::Request {
                context: "getting anime metadata".into(),
                source,
            })?;

        let resp = Self::ensure_success_or_ddg(
            resp,
            "animepahe release api",
            self.cookie_header.is_some(),
        )
        .await?;

        let doc =
            Html::parse_document(&resp.text().await.map_err(|source| PaheError::Request {
                context: "".to_string(),
                source,
            })?);

        let mut title = None;

        let sel = Selector::parse(".title-wrapper h1 span").expect("invalid selector");
        if let Some(first) = doc.select(&sel).next() {
            title = first.text().next().map(String::from);
        };

        debug!(
            anime_id = %id,
            title = title.as_deref().unwrap_or("<none>"),
            "parsed series metadata"
        );
        Ok(Anime { id, title })
    }

    /// returns the total number of episodes reported by animepahe for a series.
    pub async fn get_series_episode_count(&self, id: &str) -> Result<i32> {
        info!(anime_id = %id, "fetching series episode count");
        let url = format!(
            "https://{}/api?m=release&id={id}&sort=episode_asc&page=1",
            self.base_domain
        );

        let resp = self
            .client
            .get(url)
            .headers(self.headers(format!("https://{}/", self.base_domain).as_ref(), true))
            .send()
            .await
            .map_err(|source| PaheError::Request {
                context: "requesting animepahe release api".to_string(),
                source,
            })?;

        let resp = Self::ensure_success_or_ddg(
            resp,
            "animepahe release api",
            self.cookie_header.is_some(),
        )
        .await?;

        let parsed: ReleasePage = resp.json().await.map_err(|source| PaheError::Json {
            context: "parsing release api json".to_string(),
            source,
        })?;
        debug!(anime_id = %id, total = parsed.total, "parsed episode count");
        Ok(parsed.total)
    }

    /// collects animepahe play links for an inclusive episode range.
    ///
    /// internally this walks api pages in chunks of 30 episodes.
    pub async fn fetch_series_episode_links(
        &self,
        id: &str,
        from_episode: i32,
        to_episode: i32,
    ) -> Result<Vec<(u32, String)>> {
        let start_page = ((from_episode - 1) / 30) + 1;
        let end_page = ((to_episode - 1) / 30) + 1;
        info!(
            anime_id = %id,
            from_episode,
            to_episode,
            start_page,
            end_page,
            "fetching series episode links"
        );
        let mut links = Vec::new();

        for page in start_page..=end_page {
            debug!(page, "loading release page");
            let url = format!(
                "https://{}/api?m=release&id={id}&sort=episode_asc&page={page}",
                self.base_domain
            );

            let resp = self
                .client
                .get(url)
                .headers(self.headers(format!("https://{}/", self.base_domain).as_ref(), true))
                .send()
                .await
                .map_err(|source| PaheError::Request {
                    context: format!("loading api page {page}"),
                    source,
                })?;

            let resp = Self::ensure_success_or_ddg(
                resp,
                &format!("animepahe page {page}"),
                self.cookie_header.is_some(),
            )
            .await?;

            let parsed: ReleasePage = resp.json().await.map_err(|source| PaheError::Json {
                context: format!("parsing release page {page} json"),
                source,
            })?;
            debug!(page, entries = parsed.data.len(), "parsed release page");

            let mut current_index = (start_page - 1) * 30;

            for item in parsed.data {
                current_index += 1;

                if current_index < from_episode {
                    continue;
                }

                if current_index > to_episode {
                    break;
                }

                links.push((
                    item.episode,
                    format!("https://{}/play/{id}/{}", self.base_domain, item.session),
                ));
            }
        }

        info!(
            anime_id = %id,
            fetched_links = links.len(),
            "finished fetching series episode links"
        );
        Ok(links)
    }

    /// parses all available mirrors/qualities from a play page.
    pub async fn fetch_episode_variants(&self, play_link: &str) -> Result<Vec<EpisodeVariant>> {
        info!(%play_link, "fetching episode variants");
        let resp = self
            .client
            .get(play_link)
            .headers(self.headers(play_link, false))
            .send()
            .await
            .map_err(|source| PaheError::Request {
                context: format!("getting play page {play_link}"),
                source,
            })?;

        let resp = Self::ensure_success_or_ddg(
            resp,
            &format!("play page {play_link}"),
            self.cookie_header.is_some(),
        )
        .await?;

        let text = resp
            .text()
            .await
            .map_err(|source| PaheError::ResponseBody {
                context: "reading play page body".to_string(),
                source,
            })?;

        let doc = Html::parse_document(&text);
        let anchor_sel =
            Selector::parse(format!(r#"a[href^="https://{}"]"#, self.redirect_domain).as_ref())
                .unwrap();
        let span_sel = Selector::parse("span").unwrap();

        let mut variants = Vec::new();

        for a in doc.select(&anchor_sel) {
            let dpahe_link = a.value().attr("href").unwrap_or_default().to_string();

            let block = a.inner_html();
            let full_text = a.text().collect::<Vec<_>>().join(" ").to_lowercase();

            // resolution
            let resolution = full_text
                .split_whitespace()
                .find_map(|w| {
                    if w.ends_with('p') {
                        w.trim_end_matches('p').parse::<i32>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            // audio language
            let mut lang = "jp".to_string();

            let mut bluray = false;

            for span in a.select(&span_sel) {
                let content = span.text().collect::<String>().trim().to_lowercase();
                match content.as_str() {
                    "bd" => {
                        bluray = true;
                    }
                    "eng" => {
                        lang = "en".to_string();
                        break;
                    }
                    "chi" => {
                        lang = "zh".to_string();
                        break;
                    }
                    _ => {}
                }
            }

            variants.push(EpisodeVariant {
                dpahe_link,
                source_text: block,
                resolution,
                lang,
                bluray,
            });
            if let Some(last) = variants.last() {
                debug!(
                    dpahe_link = %last.dpahe_link,
                    resolution = last.resolution,
                    lang = %last.lang,
                    bluray = last.bluray,
                    "parsed variant"
                );
            }
        }

        if variants.is_empty() {
            info!(%play_link, "no variants found on play page");
            return Err(PaheError::NoMirrors);
        }

        info!(%play_link, variant_count = variants.len(), "finished parsing episode variants");
        Ok(variants)
    }

    pub async fn fetch_episode_index(&self, play_link: &str) -> Result<u32> {
        info!(%play_link, "fetching episode index");
        let resp = self
            .client
            .get(play_link)
            .headers(self.headers(play_link, false))
            .send()
            .await
            .map_err(|source| PaheError::Request {
                context: format!("getting play page {play_link}"),
                source,
            })?;

        let resp = Self::ensure_success_or_ddg(
            resp,
            &format!("play page {play_link}"),
            self.cookie_header.is_some(),
        )
        .await?;

        let text = resp
            .text()
            .await
            .map_err(|source| PaheError::ResponseBody {
                context: "reading play page body".to_string(),
                source,
            })?;

        let episode = Html::parse_document(&text)
            .select(&Selector::parse("button#episodeMenu").unwrap())
            .next()
            .and_then(|e| {
                e.text()
                    .collect::<String>()
                    .split_whitespace()
                    .last()?
                    .parse::<u32>()
                    .ok()
            })
            .ok_or_else(|| PaheError::Message("failed to parse episode number".into()))?;

        debug!(%play_link, episode, "parsed episode index");
        Ok(episode)
    }

    /// resolves a `pahe.win` variant into a final downloadable direct link.
    pub async fn resolve_direct_link(&self, variant: &EpisodeVariant) -> Result<DirectLink> {
        info!(dpahe_link = %variant.dpahe_link, "resolving direct link via kwik");
        let direct = self.kwik.extract_kwik_link(&variant.dpahe_link).await?;
        debug!(referer = %direct.referer, "resolved direct link");
        Ok(direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_DOMAIN: &str = "animepahe.si";

    #[test]
    fn anime_id_extracts_uuid_segment() {
        let link = format!("https://{BASE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000");
        let id = PaheClient::anime_id(&link).expect("anime id should parse");
        assert_eq!(id, "123e4567-e89b-12d3-a456-426614174000");
    }

    #[test]
    fn anime_id_rejects_non_matching_link() {
        let link = format!("https://{BASE_DOMAIN}/anime/not-a-uuid");
        let err = PaheClient::anime_id(&link).expect_err("invalid link should error");
        assert!(matches!(err, PaheError::InvalidAnimeLink { .. }));
    }

    #[test]
    fn detect_ddos_guard_matches_known_markers() {
        assert!(PaheClient::detect_ddos_guard(
            "<title>DDoS-Guard</title><p>Checking your browser before accessing</p>"
        ));
        assert!(PaheClient::detect_ddos_guard(
            "script src=\"/.well-known/ddos-guard/js-challenge\""
        ));
        assert!(!PaheClient::detect_ddos_guard("<html>normal page</html>"));
    }
}
