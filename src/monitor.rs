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
    num_newlines_last_display: Option<usize>,
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
        Self {
            seed_monitor,
            random_monitor,
            writer,
            speed_calculator: RealTimeRunningAverage::default(),
            written_bytes: 0,
            progress,
            num_newlines_last_display: None,
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

        let content = format_display_content(
            self.progress.as_ref(),
            new_written_bytes,
            current_speed_mb_s,
            num_seed_blocks,
            num_random_blocks,
        );

        // Move cursor up to overwrite previous output
        if let Some(n) = self.num_newlines_last_display {
            print!("\x1b[{}A", n);
        }
        self.num_newlines_last_display = Some(content.chars().filter(|&c| c == '\n').count());

        print!("{content}");
        io::stdout().flush()?;

        Ok(())
    }
}

fn format_display_content(
    progress: Option<&ProgressInfo>,
    new_written_bytes: u64,
    current_speed_mb_s: f64,
    num_seed_blocks: usize,
    num_random_blocks: usize,
) -> String {
    let mut speed_val = String::new();
    write!(speed_val, "{current_speed_mb_s:.2} MB/s").unwrap();

    if let Some(progress) = progress {
        let device_size = progress.device_size;
        let skip_bytes = progress.skip_bytes;

        // Overall device position = skip_bytes + bytes written this session
        let device_position = skip_bytes + new_written_bytes;
        let position_gb = (device_position as f64) / ((1024 * 1024 * 1024) as f64);
        let total_gb = (device_size as f64) / ((1024 * 1024 * 1024) as f64);
        let mut written_val = String::new();
        write!(written_val, "{position_gb:.2} / {total_gb:.2} GB").unwrap();

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

        format!(
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
        )
    } else {
        let written_gb = (new_written_bytes as f64) / ((1024 * 1024 * 1024) as f64);
        let mut written_val = String::new();
        write!(written_val, "{written_gb:.2} GB").unwrap();

        format!(
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
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_progress_content_newlines_do_not_end_with_newline() {
        let content = format_display_content(None, 0, 0.0, 0, 0);
        assert!(
            !content.ends_with('\n'),
            "Content must not end with a trailing newline, \
             otherwise the cursor position calculation would be off"
        );
    }

    #[test]
    fn progress_content_newlines_do_not_end_with_newline() {
        let progress = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        let content = format_display_content(Some(&progress), 500_000_000, 100.0, 5, 10);
        assert!(
            !content.ends_with('\n'),
            "Content must not end with a trailing newline, \
             otherwise the cursor position calculation would be off"
        );
    }

    #[test]
    fn no_progress_content_has_consistent_newlines_across_values() {
        // The number of newlines must be the same regardless of the data values,
        // since we use the newline count from the previous display to move the
        // cursor up before overwriting.
        let content_a = format_display_content(None, 0, 0.0, 0, 0);
        let content_b = format_display_content(None, 999_999_999_999, 999.99, 100, 200);
        let newlines_a = content_a.chars().filter(|&c| c == '\n').count();
        let newlines_b = content_b.chars().filter(|&c| c == '\n').count();
        assert_eq!(
            newlines_a, newlines_b,
            "Newline count must be the same for all data values in no-progress mode"
        );
    }

    #[test]
    fn progress_content_has_consistent_newlines_across_values() {
        let progress_a = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        let progress_b = ProgressInfo {
            device_size: 10_000_000_000_000,
            skip_bytes: 5_000_000_000_000,
        };
        let content_a = format_display_content(Some(&progress_a), 0, 0.0, 0, 0);
        let content_b =
            format_display_content(Some(&progress_b), 999_999_999_999, 999.99, 100, 200);
        let newlines_a = content_a.chars().filter(|&c| c == '\n').count();
        let newlines_b = content_b.chars().filter(|&c| c == '\n').count();
        assert_eq!(
            newlines_a, newlines_b,
            "Newline count must be the same for all data values in progress mode"
        );
    }
}
