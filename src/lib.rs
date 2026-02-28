//! # pahe
//!
//! A small library for AnimePahe and Kwik in Rust
//!
//! ## features
//!
//! - fetch series & episodes metadata
//! - resolve kwik mirror links
//!
//! ## usage
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

pub mod builder;
pub mod client;
pub mod errors;
pub mod prelude;
