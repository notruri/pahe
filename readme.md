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

### cli parallel downloader

`pahe-cli` now supports a wget-like parallel downloader command:

```bash
pahe-cli download --url "https://example.com/file.mp4" --connections 8
```

or resolve from animepahe first, then download:

```bash
pahe-cli download --series "https://animepahe.si/anime/<id>" --cookies "$PAHE_COOKIES" --episode 1
# output is auto-detected from Content-Disposition filename when omitted
```
