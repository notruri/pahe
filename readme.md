## pahe

small library for AnimePahe and Kwik derived from private sources, written in Rust.

### features

- fetch series & episodes metadata
- resolve kwik mirror links

### usage notes

- this project is currently in alpha, and may or may not work correctly
- this project is async and built around reqwest + tokio.
- some animepahe requests may require ddos-guard clearance cookies.
- pass cookie headers through the builder when needed.
