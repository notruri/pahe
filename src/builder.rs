use crate::{PaheClient, Result};

pub struct PaheBuilder {
    cookies: Option<String>,
}

impl PaheBuilder {
    /// creates a new builder with no cookie header configured.
    pub fn new() -> Self {
        Self { cookies: None }
    }

    /// sets a raw cookie header string used for ddos-guard clearance.
    pub fn cookies_str(mut self, cookies: &str) -> Self {
        self.cookies = Some(cookies.to_string());
        self
    }

    /// builds a [`PaheClient`] using the configured options.
    pub fn build(&self) -> Result<PaheClient> {
        if let Some(cookies) = &self.cookies {
            return PaheClient::new_with_clearance_cookie(cookies);
        }

        PaheClient::new()
    }
}
