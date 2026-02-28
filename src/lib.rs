//! # pahe
//!
//! small library for AnimePahe and Kwik in Rust
//!
//! ## features
//!
//! - fetch series & episodes metadata
//! - resolve kwik mirror links
//! - allows downloading of episodes
//! - concurrent requests when downloading
//!
//! ## usage
//!
//! ### client
//!
//! ```rust
//! use pahe::prelude::*;
//!
//! #[tokio::main]
//! async fn main() {
//!     let cookies = "__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;";
//!     let pahe = PaheBuilder::new()
//!         .cookies_str(cookies)
//!         .build()
//!         .unwrap();
//!     let anime = pahe.get_series_metadata("https://animepahe.si/anime/8d9c277c-d8eb-f789-6158-b853a7236f14").await.unwrap();
//!     println!("{:?}", anime);
//! }
//! ```
//!
//! ### cli
//!
//! #### downloading
//!
//! - series
//!
//!     if you wish to download all the episodes from a series
//!
//!     ```bash
//!     pahe-cli download --series https://animepahe.si/anime/8d9c277c-d8eb-f789-6158-b853a7236f14 --quality highest --dir downloads
//!     ```
//!
//! - specific episodes
//!
//!     or if you prefer to download specific episodes
//!
//!     ```bash
//!     pahe-cli download --series https://animepahe.si/anime/8d9c277c-d8eb-f789-6158-b853a7236f14 --episodes 1-12 --quality highest --dir downloads
//!     ```
//!
//!     or if you want to only download a single episode
//!
//!     ```bash
//!     pahe-cli download --series https://animepahe.si/anime/8d9c277c-d8eb-f789-6158-b853a7236f14 --episodes 16
//!     ```
//!
//! #### interactive mode
//!
//! or if you don't want to manually type arguments, use interactive mode using `-i` or `--interactive` flag
//!
//! ```bash
//! pahe-cli -i
//! ```
//!
//! #### bypassing ddos-guard
//!
//! AnimePahe has a ddos-guard to prevent spamming, if ddos-guard blocks the request, go to the animepahe website, copy the cookies and either set the `PAHE_COOKIES` environment variable or pass them into the `--cookies` flag
//!
//! - using environment variable
//!
//!     ```bash
//!     export PAHE_COOKIES='__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;'
//!     ```
//!
//! - using command line argument
//!
//!     ```bash
//!     pahe-cli --cookies '__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;'
//!     ```
//!
//! ### usage notes
//!
//! - this project is currently in alpha, and it may or may not work correctly
//! - some animepahe requests may require ddos-guard clearance cookies.
//! - pass cookie headers through the builder when needed.

pub mod builder;
pub mod client;
pub mod errors;
pub mod prelude;
