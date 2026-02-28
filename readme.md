<h1 align="center">
    <img src="assets/pahe.png" alt="Pahe" height="100">
</h1>
<p align="center">A small library and CLI to interact with AnimePahe and Kwik in Rust</p>

<br>

## features

- fetch series & episodes metadata
- resolve kwik mirror links
- concurrent downloads

## supported platforms

- Linux (x86_64)
- Windows (x86_64)
- macOS (i cant fully verify this)

## usage

### client

```rust
use pahe::prelude::*;

#[tokio::main]
async fn main() {
    let cookies = "__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;";
    let pahe = PaheBuilder::new()
        .cookies_str(cookies)
        .build()
        .unwrap();
    let anime = pahe.get_series_metadata("https://animepahe.si/anime/8d9c277c-d8eb-f789-6158-b853a7236f14").await.unwrap();
    println!("{:?}", anime);
}
```

### cli

#### downloading

an episode

![Downloading an episode](assets/downloading.gif)

```bash
pahe-cli download \
    --series https://animepahe.si/anime/4a9abc55-0a54-c544-3e14-736c79ddafe7 \
    --episodes 1
```

specific episodes

![Downloading episodes](assets/batch.gif)

```bash
pahe-cli download \
    --series https://animepahe.si/anime/4a9abc55-0a54-c544-3e14-736c79ddafe7 \
    --episodes 2-5
```

#### interactive mode

or if you don't want to manually type arguments, use interactive mode using `-i` or `--interactive` flag

![Interactive mode](assets/interactive.gif)

```bash
pahe-cli -i
```

#### bypassing ddos-guard

AnimePahe has a ddos-guard to prevent spamming, if ddos-guard blocks the request, go to the animepahe website, copy the cookies and either set the `PAHE_COOKIES` environment variable or pass them into the `--cookies` flag

- using environment variable

    ```bash
    export PAHE_COOKIES='__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;'
    ```

- using command line argument

    ```bash
    pahe-cli --cookies '__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;'
    ```

### usage notes

- this project is currently in alpha, and it may or may not work correctly
- some animepahe requests may require ddos-guard clearance cookies.
- pass cookie headers through the builder when needed.
- if parallel downloads aren't working (eg; stalling), try reducing the connections or set it to single connection (`-n 1`)
