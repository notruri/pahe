# pahe-rs

small rust library for animepahe episode discovery and kwik direct-link extraction.

## features

- fetch episode counts for a series
- collect play links for an episode range
- parse available mirror variants from play pages
- select variants by language and resolution preference
- resolve pahe mirror links into direct downloadable urls

## usage notes

- this project is async and built around reqwest + tokio.
- some animepahe requests may require ddos-guard clearance cookies.
- pass cookie headers through the builder when needed.

## development

```bash
cargo fmt
cargo check
```
