use pahe::PaheBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pahe = PaheBuilder::new()
        .cookies_str(
            "__ddgid_=VGWtUB15hlasBLCE; __ddg2_=kGckOKa1z5a2I7yi; __ddg1_=UgXYjtJdbr7gS8ZiQH8z;",
        )
        .build()?;

    let series = "https://animepahe.si/anime/4a9abc55-0a54-c544-3e14-736c79ddafe7";
    let count = pahe.get_series_episode_count(series).await?;

    let play_links = pahe
        .fetch_series_episode_links(series, 1, count.min(2))
        .await?;
    let variants = pahe.fetch_episode_variants(&play_links[0]).await?;

    println!("variants: {variants:#?}");

    let selected = pahe.select_variant(variants, 0, "jp")?;

    println!("selected: {selected:#?}");

    let resolved = pahe.resolve_direct_link(&selected).await?;

    println!("resolved: {}", resolved.direct_link);
    Ok(())
}
