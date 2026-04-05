use anyhow::Result;
use colored::Colorize;
use running_average::RealTimeRunningAverage;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};

use crate::block_writer::BlockWriter;
use crate::producer::ProductReceiver;

const CLEAR_LINE: &str = "\x1b[2K";
const PROGRESS_BAR_WIDTH: usize = 30;

pub struct ProgressInfo {
    /// Total size of the device in bytes.
    pub device_size: u64,
    /// Number of bytes skipped at the start (from --skip-bytes).
    pub skip_bytes: u64,
}

pub struct Monitor<'w, M1, M2>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<Vec<u8>>,
{
    seed_monitor: M1,
    random_monitor: M2,
    writer: &'w BlockWriter,
    speed_calculator: RealTimeRunningAverage<f64>,
    written_bytes: u64,
    progress: Option<ProgressInfo>,
    has_displayed: bool,
    num_display_lines: usize,
}

impl<'w, M1, M2> Monitor<'w, M1, M2>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<Vec<u8>>,
{
    pub fn new(
        seed_monitor: M1,
        random_monitor: M2,
        writer: &'w BlockWriter,
        progress: Option<ProgressInfo>,
    ) -> Self {
        let num_display_lines = if progress.is_some() { 7 } else { 5 };
        Self {
            seed_monitor,
            random_monitor,
            writer,
            speed_calculator: RealTimeRunningAverage::default(),
            written_bytes: 0,
            progress,
            has_displayed: false,
            num_display_lines,
        }
    }

    pub fn display(&mut self) -> Result<()> {
        let new_written_bytes = self.writer.num_bytes_written();
        self.speed_calculator
            .insert((new_written_bytes - self.written_bytes) as f64);
        self.written_bytes = new_written_bytes;

        let current_speed_mb_s =
            self.speed_calculator.measurement().rate() / ((1024 * 1024) as f64);
        let num_seed_blocks = self.seed_monitor.num_products_in_buffer();
        let num_random_blocks = self.random_monitor.num_products_in_buffer();

        // Move cursor up to overwrite previous output
        if self.has_displayed {
            print!("\x1b[{}A", self.num_display_lines);
        }
        self.has_displayed = true;

        let mut written_val = String::new();
        let mut speed_val = String::new();
        write!(speed_val, "{current_speed_mb_s:.2} MB/s")?;

        if let Some(ref progress) = self.progress {
            let device_size = progress.device_size;
            let skip_bytes = progress.skip_bytes;

            // Overall device position = skip_bytes + bytes written this session
            let device_position = skip_bytes + new_written_bytes;
            let position_gb = (device_position as f64) / ((1024 * 1024 * 1024) as f64);
            let total_gb = (device_size as f64) / ((1024 * 1024 * 1024) as f64);
            write!(written_val, "{position_gb:.2} / {total_gb:.2} GB")?;

            // Progress bar reflects overall device completion
            let fraction = if device_size > 0 {
                (device_position as f64) / (device_size as f64)
            } else {
                1.0
            };
            let percentage = (fraction * 100.0).min(100.0);
            let filled = ((fraction * PROGRESS_BAR_WIDTH as f64) as usize).min(PROGRESS_BAR_WIDTH);
            let empty = PROGRESS_BAR_WIDTH - filled;

            let bar = format!(
                "[{}{}] {percentage:>5.1}%",
                "\u{2588}".repeat(filled),
                "\u{2591}".repeat(empty),
            );

            // ETA based on remaining bytes to write this session
            let remaining_bytes = device_size.saturating_sub(device_position);
            let eta = if current_speed_mb_s > 0.0 {
                let remaining_secs =
                    (remaining_bytes as f64) / (current_speed_mb_s * (1024 * 1024) as f64);
                format_duration(remaining_secs)
            } else {
                "--:--:--".to_string()
            };

            print!(
                "{CLEAR_LINE}\n\
                 {CLEAR_LINE}  {label_progress}  {bar}\n\
                 {CLEAR_LINE}  {label_written}  {written}\n\
                 {CLEAR_LINE}  {label_speed}  {speed}  {label_eta}  {eta}\n\
                 {CLEAR_LINE}\n\
                 {CLEAR_LINE}  {label_seed}  {seed} blocks\n\
                 {CLEAR_LINE}  {label_random}  {random} blocks",
                label_progress = format!("{:>14}", "Progress:").bold(),
                bar = bar.cyan().bold(),
                label_written = format!("{:>14}", "Written:").bold(),
                written = written_val.cyan().bold(),
                label_speed = format!("{:>14}", "Speed:").bold(),
                speed = format!("{:>10}", speed_val).green(),
                label_eta = "ETA:".bold(),
                eta = eta.yellow(),
                label_seed = format!("{:>14}", "Seed buffer:").dimmed(),
                seed = format!("{num_seed_blocks:>4}").dimmed(),
                label_random = format!("{:>14}", "Random buffer:").dimmed(),
                random = format!("{num_random_blocks:>4}").dimmed(),
            );
        } else {
            let written_gb = (new_written_bytes as f64) / ((1024 * 1024 * 1024) as f64);
            write!(written_val, "{written_gb:.2} GB")?;

            print!(
                "{CLEAR_LINE}\n\
                 {CLEAR_LINE}  {label_written}  {written}\n\
                 {CLEAR_LINE}  {label_speed}  {speed}\n\
                 {CLEAR_LINE}\n\
                 {CLEAR_LINE}  {label_seed}  {seed} blocks\n\
                 {CLEAR_LINE}  {label_random}  {random} blocks",
                label_written = format!("{:>14}", "Written:").bold(),
                written = format!("{:>10}", written_val).cyan().bold(),
                label_speed = format!("{:>14}", "Speed:").bold(),
                speed = format!("{:>10}", speed_val).green(),
                label_seed = format!("{:>14}", "Seed buffer:").dimmed(),
                seed = format!("{num_seed_blocks:>4}").dimmed(),
                label_random = format!("{:>14}", "Random buffer:").dimmed(),
                random = format!("{num_random_blocks:>4}").dimmed(),
            );
        }
        io::stdout().flush()?;

        Ok(())
    }
}

fn format_duration(secs: f64) -> String {
    if secs.is_infinite() || secs.is_nan() || secs < 0.0 {
        return "--:--:--".to_string();
    }
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
