use crate::{PaheClient, Result};

pub struct PaheBuilder {
    cookies: Option<String>,
}

impl PaheBuilder {
    pub fn new() -> Self {
        Self { cookies: None }
    }

    pub fn cookies_str(mut self, cookies: &str) -> Self {
        self.cookies = Some(cookies.to_string());
        self
    }

    pub fn build(&self) -> Result<PaheClient> {
        if let Some(cookies) = &self.cookies {
            return PaheClient::new_with_clearance_cookie(cookies);
        }

        PaheClient::new()
    }
}
