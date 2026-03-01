use std::io::Write;
use std::time::{Duration, Instant};

use crossterm::{cursor::*, execute, style::*, terminal::*};
use owo_colors::OwoColorize;
use pahe_downloader::DownloadEvent;

use crate::utils::*;

pub struct DownloadProgressRenderer {
    enabled: bool,
    initialized: bool,
    spinner_step: usize,
    started_at: Option<Instant>,
    downloaded: u64,
    finished: bool,
    total: Option<u64>,
    status: DownloadStatus,
}

#[derive(Debug, Clone, Copy)]
enum DownloadStatus {
    Waiting,
    Downloading,
    Done,
}

impl DownloadProgressRenderer {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            initialized: false,
            spinner_step: 0,
            started_at: None,
            downloaded: 0,
            finished: false,
            total: None,
            status: DownloadStatus::Waiting,
        }
    }

    pub fn handle(&mut self, event: DownloadEvent) {
        if !self.enabled {
            return;
        }

        match event {
            DownloadEvent::Started { total_bytes, .. } => {
                self.total = total_bytes;
                self.downloaded = 0;
                self.finished = false;
                self.started_at = Some(Instant::now());
                self.status = DownloadStatus::Waiting;
                self.draw_current();
            }
            DownloadEvent::Progress {
                downloaded_bytes,
                total_bytes,
                elapsed,
            } => {
                self.total = total_bytes;
                self.downloaded = downloaded_bytes;
                self.started_at = Some(Instant::now() - elapsed);
                self.finished = false;
                self.status = DownloadStatus::Downloading;
                self.draw_current();
            }
            DownloadEvent::Finished {
                downloaded_bytes,
                elapsed,
            } => {
                self.downloaded = downloaded_bytes;
                self.started_at = Some(Instant::now() - elapsed);
                self.finished = true;
                self.status = DownloadStatus::Done;
                self.draw_current();
            }
        }
    }

    pub fn tick(&mut self) {
        if !self.enabled || self.finished || self.started_at.is_none() {
            return;
        }
        self.draw_current();
    }

    fn draw_current(&mut self) {
        let elapsed = self
            .started_at
            .map(|started| started.elapsed())
            .unwrap_or(Duration::ZERO);
        self.draw_frame(self.downloaded, self.total, elapsed, self.finished);
    }

    pub fn draw_frame(
        &mut self,
        downloaded: u64,
        total: Option<u64>,
        elapsed: Duration,
        done: bool,
    ) {
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

        let bar_width = 43.0;
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
        let status_text = match self.status {
            DownloadStatus::Waiting => "waiting",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Done => "done",
        };

        let status_cell = fit_cell(status_text, 13, false);
        let downloaded_cell = fit_cell(&downloaded_text, 13, true);
        let total_cell = fit_cell(&total_text, 13, false);
        let speed_cell = fit_cell(&speed_text, 16, true);

        let spinner = format!("[{spinner}]").cyan();
        let bar = bar.green();
        let status_cell = status_cell.blue();
        let downloaded_cell = downloaded_cell.yellow();
        let total_cell = total_cell.dimmed();
        let speed_cell = speed_cell.cyan();
        let eta_text = eta_text.magenta();

        let _ = execute!(stdout, MoveUp(3), Clear(ClearType::FromCursorDown));
        let _ = writeln!(stdout);
        let _ = writeln!(stdout, "{spinner} {bar}  eta {eta_text}");
        let _ = writeln!(
            stdout,
            "{status_cell} {downloaded_cell} / {total_cell} {speed_cell}"
        );
        let _ = stdout.flush();
    }
}

fn fit_cell(text: &str, width: usize, align_right: bool) -> String {
    let clipped = if text.len() > width {
        text[..width].to_string()
    } else {
        text.to_string()
    };

    if align_right {
        format!("{clipped:>width$}")
    } else {
        format!("{clipped:<width$}")
    }
}
