use std::io::Write;
use std::time::Duration;

use crossterm::{cursor::*, execute, style::*, terminal::*};
use owo_colors::OwoColorize;
use pahe_downloader::DownloadEvent;

use crate::utils::*;

pub struct DownloadProgressRenderer {
    enabled: bool,
    initialized: bool,
    spinner_step: usize,
    total: Option<u64>,
}

impl DownloadProgressRenderer {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            initialized: false,
            spinner_step: 0,
            total: None,
        }
    }

    pub fn handle(&mut self, event: DownloadEvent) {
        if !self.enabled {
            return;
        }

        match event {
            DownloadEvent::Started { total_bytes, .. } => {
                self.total = total_bytes;
                self.draw_frame(0, total_bytes, Duration::ZERO, false);
            }
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                elapsed,
            } => {
                self.total = total_bytes;
                self.draw_frame(downloaded_bytes, total_bytes, elapsed, false);
            }
            DownloadEvent::Finished {
                downloaded_bytes,
                elapsed,
            } => {
                self.draw_frame(downloaded_bytes, self.total, elapsed, true);
            }
        }
    }

    pub fn draw_frame(&mut self, downloaded: u64, total: Option<u64>, elapsed: Duration, done: bool) {
        let mut stdout = std::io::stdout();

        if !self.initialized {
            let _ = writeln!(stdout);
            let _ = writeln!(stdout);
            self.initialized = true;
        }

        let spinner = if done {
            "✓"
        } else {
            const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame = FRAMES[self.spinner_step % FRAMES.len()];
            self.spinner_step = self.spinner_step.wrapping_add(1);
            frame
        };

        let ratio = total
            .map(|total_bytes| {
                if total_bytes == 0 {
                    1.0
                } else {
                    downloaded as f64 / total_bytes as f64
                }
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);

        let bar_width = 45.0;
        let filled = (ratio * bar_width).round();
        let empty = bar_width - filled;
        let bar = format!(
            "[{}{}]",
            "█".repeat(filled as usize),
            " ".repeat(empty as usize)
        );

        let speed_bps = if elapsed.as_secs_f64() > 0.0 {
            downloaded as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let speed_text = format!("{}/s", format_bytes_f64(speed_bps));

        let eta = total.and_then(|total_bytes| estimate_eta(downloaded, total_bytes, elapsed));
        let downloaded_text = format_bytes(downloaded);
        let total_text = total
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        let eta_text = eta
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string());

        let spinner = spinner.cyan();
        let bar = bar.green();
        let downloaded_text = downloaded_text.yellow();
        let total_text = total_text.dimmed();
        let eta_text = eta_text.magenta();

        let _ = execute!(stdout, MoveUp(2), Clear(ClearType::CurrentLine));
        let _ = writeln!(stdout, "[{spinner}] {bar}  eta {eta_text}");
        let _ = writeln!(
            stdout,
            "{downloaded_text:>14} / {total_text:<14}  {speed_text:>30}"
        );
        let _ = stdout.flush();
    }
}
