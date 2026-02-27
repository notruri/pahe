use crate::prelude::*;

const BASE_DOMAIN: &str = "animepahe.si";

pub struct PaheBuilder {
    base_domain: String,
    cookies: Option<String>,
}

impl PaheBuilder {
    /// creates a new builder with no cookie header configured.
    pub fn new() -> Self {
        Self { base_domain: BASE_DOMAIN.to_string(), cookies: None }
    }

    /// sets a raw cookie header string used for ddos-guard clearance.
    pub fn cookies_str(mut self, cookies: &str) -> Self {
        self.cookies = Some(cookies.to_string());
        self
    }
    
    /// sets the base domain for the client.
    pub fn base_domain(mut self, domain: &str) -> Self {
        self.base_domain = domain.to_string();
        self
    }

    /// builds a [`PaheClient`] using the configured options.
    pub fn build(&self) -> Result<PaheClient> {
        if let Some(cookies) = &self.cookies {
            return PaheClient::new_with_clearance_cookie(self.base_domain.clone(), cookies);
        }

        PaheClient::new(self.base_domain.clone())
    }
}
