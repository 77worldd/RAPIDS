use std::io::{IsTerminal, Write};

use rapids_engine::util::{human_bytes, human_duration};
use rapids_engine::EngineEvent;

/// Terminal presentation for the event stream: one live progress line when
/// attached to a terminal, plain lines for everything else.
pub struct LineWriter {
    progress_live: bool,
    terminal: bool,
}

impl LineWriter {
    pub fn new() -> Self {
        Self {
            progress_live: false,
            terminal: std::io::stdout().is_terminal(),
        }
    }

    pub fn write(&mut self, event: &EngineEvent) {
        match event {
            EngineEvent::Progress {
                downloaded,
                total,
                speed_bps,
                eta_secs,
                active_connections,
                segments_done,
                segments_total,
            } => {
                if !self.terminal {
                    return;
                }
                let line = progress_line(
                    *downloaded,
                    *total,
                    *speed_bps,
                    *eta_secs,
                    *active_connections,
                    *segments_done,
                    *segments_total,
                );
                print!("\r{line}\x1b[K");
                let _ = std::io::stdout().flush();
                self.progress_live = true;
            }
            other => {
                self.finish_line();
                println!("{other}");
            }
        }
    }

    /// Ends a live progress line so the next message starts clean.
    pub fn finish_line(&mut self) {
        if self.progress_live {
            println!();
            self.progress_live = false;
        }
    }
}

impl Default for LineWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LineWriter {
    fn drop(&mut self) {
        self.finish_line();
    }
}

fn progress_line(
    downloaded: u64,
    total: Option<u64>,
    speed_bps: u64,
    eta_secs: Option<u64>,
    active_connections: usize,
    segments_done: usize,
    segments_total: usize,
) -> String {
    let percent = match total {
        Some(value) if value > 0 => format!("{:.1}%", downloaded as f64 * 100.0 / value as f64),
        Some(_) => "100.0%".to_string(),
        None => "--.-%".to_string(),
    };
    let total_text = match total {
        Some(value) => human_bytes(value),
        None => "unknown".to_string(),
    };
    let eta_text = match eta_secs {
        Some(secs) => human_duration(secs),
        None => "--:--".to_string(),
    };
    format!(
        "{percent}  {}/{total_text} · {}/s · ETA {eta_text} · {active_connections} connections · segments {segments_done}/{segments_total}",
        human_bytes(downloaded),
        human_bytes(speed_bps),
    )
}
