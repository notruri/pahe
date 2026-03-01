use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, execute};
use owo_colors::OwoColorize;
use std::io::Write;
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

use pahe::errors::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct CliLogger {
    pub level: LogLevel,
    pub spinner_step: AtomicUsize,
    pub loading_active: AtomicBool,
    pub loading_padded: AtomicBool,
}

#[derive(Debug, Clone, Copy)]
enum LogState {
    Success,
    Failed,
    Debug,
}

impl CliLogger {
    pub fn new(level: &str) -> Self {
        Self::new_(level).unwrap_or(CliLogger {
            level: LogLevel::Info,
            spinner_step: AtomicUsize::new(0),
            loading_active: AtomicBool::new(false),
            loading_padded: AtomicBool::new(false),
        })
    }

    fn new_(level: &str) -> Result<Self> {
        let level = LogLevel::parse(level).ok_or(PaheError::Message(format!(
            "invalid log level: {level}. expected one of: error, warn, info, debug"
        )))?;

        Ok(Self {
            level,
            spinner_step: AtomicUsize::new(0),
            loading_active: AtomicBool::new(false),
            loading_padded: AtomicBool::new(false),
        })
    }

    fn log(&self, level: LogLevel, state: LogState, message: impl AsRef<str>) {
        self.clear_loading_line_if_needed();
        let icon = self.icon(state);

        if level <= self.level {
            println!("{} {}", icon, message.as_ref());
        }
    }

    pub fn loading(&self, message: impl AsRef<str>) {
        if LogLevel::Info > self.level {
            return;
        }

        self.draw_loading_frame(message.as_ref());
    }

    pub fn success(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Info, LogState::Success, message);
    }

    pub fn failed(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Error, LogState::Failed, message);
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        self.log(LogLevel::Debug, LogState::Debug, message);
    }

    fn icon(&self, state: LogState) -> Box<dyn std::fmt::Display> {
        match state {
            LogState::Success => Box::new("[✓]".green()),
            LogState::Failed => Box::new("[✗]".red()),
            LogState::Debug => Box::new("[λ]".purple()),
        }
    }

    pub async fn while_loading<F, T>(&self, message: impl Into<String>, future: F) -> T
    where
        F: Future<Output = T>,
    {
        if LogLevel::Info > self.level {
            return future.await;
        }

        let message = message.into();
        let mut ticker = tokio::time::interval(Duration::from_millis(120));
        let mut future = Box::pin(future);
        self.loading_active.store(true, Ordering::Relaxed);

        loop {
            tokio::select! {
                result = &mut future => {
                    self.clear_loading_line_if_needed();
                    return result;
                }
                _ = ticker.tick() => {
                    self.draw_loading_frame(&message);
                }
            }
        }
    }

    fn draw_loading_frame(&self, message: &str) {
        const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = self.spinner_step.fetch_add(1, Ordering::Relaxed);
        let frame = format!("[{}]", FRAMES[idx % FRAMES.len()]);
        let frame = frame.yellow();

        let mut stdout = std::io::stdout();

        if !self.loading_padded.swap(true, Ordering::Relaxed) {
            let _ = writeln!(stdout);
        }

        self.loading_active.store(true, Ordering::Relaxed);
        let _ = execute!(
            stdout,
            cursor::MoveToColumn(0),
            Clear(ClearType::CurrentLine)
        );
        let _ = write!(stdout, "{frame} {message}");
        let _ = stdout.flush();
    }

    fn clear_loading_line_if_needed(&self) {
        if self.loading_active.swap(false, Ordering::Relaxed) {
            let mut stdout = std::io::stdout();
            let _ = execute!(
                stdout,
                cursor::MoveToColumn(0),
                Clear(ClearType::CurrentLine)
            );
            if self.loading_padded.load(Ordering::Relaxed) {
                let _ = execute!(
                    stdout,
                    cursor::MoveUp(1),
                    cursor::MoveToColumn(0),
                    Clear(ClearType::CurrentLine)
                );
            }
            let _ = stdout.flush();
            self.loading_padded.store(false, Ordering::Relaxed);
        }
    }
}
