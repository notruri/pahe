use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;

use pahe::prelude::*;
use pahe_downloader::*;

use crate::args::*;
use crate::episode::*;
use crate::logger::*;
use crate::progress::*;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    
    #[command(flatten)]
    pub download_args: DownloadArgs,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Resolve and print a direct episode download URL
    Resolve(ResolveArgs),
    /// Download a file URL in parallel (wget-like)
    Download(DownloadArgs),
}

#[derive(Debug)]
pub struct App {
    cli: Cli,
    logger: CliLogger,
}

impl App {
    pub fn new() -> Self {
        let cli = Cli::parse();
        let log_level = match &cli.command {
            Some(Commands::Resolve(args)) => &args.app_args.log_level,
            Some(Commands::Download(args)) => &args.resolve.app_args.log_level,
            None => &cli.download_args.resolve.app_args.log_level,
        };
        let logger = CliLogger::new(log_level);
        Self { cli, logger }
    }

    pub async fn run(&self) {
        if let Err(err) = match &self.cli.command {
            Some(Commands::Resolve(args)) => self.resolve(args.clone()).await,
            Some(Commands::Download(args)) => self.download(args.clone()).await,
            None => self.download(self.cli.download_args.clone()).await,
        } {
            self.logger.failed(format!("{err}"));
        }
    }

    pub async fn resolve(&self, args: ResolveArgs) -> Result<()> {
        let logger = &self.logger;
        let resolves = resolve_episode_urls(args, logger).await?;

        logger.success("episodes has been resolved successfully");
        for (i, episode_url) in resolves.iter().enumerate() {
            logger.success(format!("episode {}: {}", i + 1, episode_url.url.yellow()));
        }

        Ok(())
    }

    pub async fn download(&self, args: DownloadArgs) -> Result<()> {
        let logger = &self.logger;

        let urls = resolve_episode_urls(args.resolve, logger).await?;

        for episode_url in urls {
            let file_name: PathBuf = match &args.output {
                Some(path) => path.into(),
                None => {
                    let guessed = logger
                        .while_loading(
                            "inferring output filename",
                            suggest_filename(&episode_url.referer, &episode_url.url),
                        )
                        .await
                        .map_err(|err| {
                            PaheError::Message(format!("failed to infer output filename: {err}"))
                        })?;
                    guessed.into()
                }
            };

            let output = match &args.dir {
                Some(dir) => dir.join(file_name),
                None => file_name,
            };

            let output_str = output.to_string_lossy().into_owned();
            let mut progress_renderer =
                DownloadProgressRenderer::new(logger.level >= LogLevel::Info);
            let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tick = tokio::time::interval(Duration::from_millis(80));
            let mut download_fut = std::pin::pin!(download(
                DownloadRequest::new(episode_url.referer, episode_url.url, output)
                    .connections(args.connections),
                move |event| {
                    let _ = events_tx.send(event);
                },
            ));

            let download_result = loop {
                tokio::select! {
                    result = &mut download_fut => break result,
                    maybe_event = events_rx.recv() => {
                        if let Some(event) = maybe_event {
                            progress_renderer.handle(event);
                        }
                    }
                    _ = tick.tick() => {
                        progress_renderer.tick();
                    }
                }
            };

            while let Ok(event) = events_rx.try_recv() {
                progress_renderer.handle(event);
            }

            download_result.map_err(|err| PaheError::Message(format!("download failed: {err}")))?;
            logger.success(format!("done {}", output_str.yellow()));
        }

        logger.success("download complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::constants::*;
    use crate::utils::*;

    #[test]
    fn normalize_series_link_accepts_anime_link() {
        let input =
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000");
        let normalized = normalize_series_link(&input).expect("anime link should be valid");
        assert_eq!(
            normalized,
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn normalize_series_link_accepts_play_link() {
        let input = format!(
            "https://{ANIMEPAHE_DOMAIN}/play/123e4567-e89b-12d3-a456-426614174000/3cf1e5860ff5e9f766b36241c4dd6d48de3ef45d41183ecd079e1772aeb27c3c"
        );
        let normalized = normalize_series_link(&input).expect("play link should be valid");
        assert_eq!(
            normalized,
            format!("https://{ANIMEPAHE_DOMAIN}/anime/123e4567-e89b-12d3-a456-426614174000")
        );
    }

    #[test]
    fn normalize_series_link_rejects_non_animepahe_links() {
        let err =
            normalize_series_link("https://example.com/anime/123e4567-e89b-12d3-a456-426614174000")
                .expect_err("non animepahe links should be rejected");
        assert!(
            err.to_string()
                .contains("invalid --series URL: expected AnimePahe")
        );
    }
}
