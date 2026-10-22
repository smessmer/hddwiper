use anyhow::Result;
use colored::Colorize;
use running_average::RealTimeRunningAverage;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};

use crate::block_pool::PooledBlock;
use crate::block_verifier::MismatchesSoFar;
use crate::producer::ProductReceiver;

const CLEAR_LINE: &str = "\x1b[2K";
const PROGRESS_BAR_WIDTH: usize = 30;

pub struct ProgressInfo {
    /// Total size of the device in bytes.
    pub device_size: u64,
    /// Number of bytes skipped at the start (from --skip-bytes).
    pub skip_bytes: u64,
}

pub struct Monitor<M1, M2, F, G>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<PooledBlock>,
    F: Fn() -> u64,
    G: Fn() -> MismatchesSoFar,
{
    /// Not everything we monitor produces seeds, so there isn't always a seed buffer to show
    seed_monitor: Option<M1>,
    random_monitor: M2,
    /// What the display calls the bytes that were processed, e.g. "Written:"
    processed_label: &'static str,
    /// Returns the number of bytes that were processed so far, e.g. written to the device
    num_bytes_processed: F,
    /// Returns how much of the processed data differs from what it should be. Only what
    /// compares data has this to show.
    mismatches: Option<G>,
    speed_calculator: RealTimeRunningAverage<f64>,
    processed_bytes: u64,
    progress: Option<ProgressInfo>,
    num_newlines_last_display: Option<usize>,
}

impl<M1, M2, F, G> Monitor<M1, M2, F, G>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<PooledBlock>,
    F: Fn() -> u64,
    G: Fn() -> MismatchesSoFar,
{
    pub fn new(
        seed_monitor: Option<M1>,
        random_monitor: M2,
        processed_label: &'static str,
        num_bytes_processed: F,
        mismatches: Option<G>,
        progress: Option<ProgressInfo>,
    ) -> Self {
        Self {
            seed_monitor,
            random_monitor,
            processed_label,
            num_bytes_processed,
            mismatches,
            speed_calculator: RealTimeRunningAverage::default(),
            processed_bytes: 0,
            progress,
            num_newlines_last_display: None,
        }
    }

    pub fn display(&mut self) -> Result<()> {
        let new_written_bytes = (self.num_bytes_processed)();
        self.speed_calculator
            .insert((new_written_bytes - self.processed_bytes) as f64);
        self.processed_bytes = new_written_bytes;

        let current_speed_mb_s =
            self.speed_calculator.measurement().rate() / ((1024 * 1024) as f64);
        let num_seed_blocks = self
            .seed_monitor
            .as_ref()
            .map(|seed_monitor| seed_monitor.num_products_in_buffer());
        let num_random_blocks = self.random_monitor.num_products_in_buffer();
        let mismatches = self.mismatches.as_ref().map(|mismatches| mismatches());

        let content = format_display_content(
            self.progress.as_ref(),
            self.processed_label,
            new_written_bytes,
            current_speed_mb_s,
            num_seed_blocks,
            num_random_blocks,
            mismatches.as_ref(),
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
    processed_label: &str,
    new_written_bytes: u64,
    current_speed_mb_s: f64,
    num_seed_blocks: Option<usize>,
    num_random_blocks: usize,
    mismatches: Option<&MismatchesSoFar>,
) -> String {
    let mut speed_val = String::new();
    write!(speed_val, "{current_speed_mb_s:.2} MB/s").unwrap();

    let mismatched = mismatches.map(format_mismatches).unwrap_or_default();

    let mut buffers = String::new();
    if let Some(num_seed_blocks) = num_seed_blocks {
        writeln!(
            buffers,
            "{CLEAR_LINE}  {label_seed}  {seed} blocks",
            label_seed = format!("{:>14}", "Seed buffer:").dimmed(),
            seed = format!("{num_seed_blocks:>4}").dimmed(),
        )
        .unwrap();
    }
    write!(
        buffers,
        "{CLEAR_LINE}  {label_random}  {random} blocks",
        label_random = format!("{:>14}", "Random buffer:").dimmed(),
        random = format!("{num_random_blocks:>4}").dimmed(),
    )
    .unwrap();

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
             {mismatched}\
             {CLEAR_LINE}  {label_speed}  {speed}  {label_eta}  {eta}\n\
             {CLEAR_LINE}\n\
             {buffers}",
            label_progress = format!("{:>14}", "Progress:").bold(),
            bar = bar.cyan().bold(),
            label_written = format!("{:>14}", processed_label).bold(),
            written = written_val.cyan().bold(),
            label_speed = format!("{:>14}", "Speed:").bold(),
            speed = format!("{:>10}", speed_val).green(),
            label_eta = "ETA:".bold(),
            eta = eta.yellow(),
        )
    } else {
        let written_gb = (new_written_bytes as f64) / ((1024 * 1024 * 1024) as f64);
        let mut written_val = String::new();
        write!(written_val, "{written_gb:.2} GB").unwrap();

        format!(
            "{CLEAR_LINE}\n\
             {CLEAR_LINE}  {label_written}  {written}\n\
             {mismatched}\
             {CLEAR_LINE}  {label_speed}  {speed}\n\
             {CLEAR_LINE}\n\
             {buffers}",
            label_written = format!("{:>14}", processed_label).bold(),
            written = format!("{:>10}", written_val).cyan().bold(),
            label_speed = format!("{:>14}", "Speed:").bold(),
            speed = format!("{:>10}", speed_val).green(),
        )
    }
}

/// The line that says how much of the data differs from what it should be, and where that
/// begins. That is what somebody wants to know as soon as it is found, and not only at the end.
fn format_mismatches(mismatches: &MismatchesSoFar) -> String {
    let value = match mismatches.first_mismatch_position {
        None => "0 bytes".green(),
        Some(first_position) => {
            let size = in_gb(mismatches.num_bytes_mismatched)
                .unwrap_or_else(|| format!("{} bytes", mismatches.num_bytes_mismatched));
            let parts = if mismatches.num_mismatches == 1 {
                "part"
            } else {
                "parts"
            };
            let first_position =
                in_gb(first_position).unwrap_or_else(|| format!("byte {first_position}"));
            format!(
                "{size} in {} {parts}, the first at {first_position}",
                mismatches.num_mismatches
            )
            .red()
            .bold()
        }
    };
    format!(
        "{CLEAR_LINE}  {label}  {value}\n",
        label = format!("{:>14}", "Mismatched:").bold(),
    )
}

/// `num_bytes` in GB, like the other sizes we show. This returns `None` if the number is too
/// small for that to say anything, because it would be shown as 0.00 GB.
fn in_gb(num_bytes: u64) -> Option<String> {
    let gb = (num_bytes as f64) / ((1024 * 1024 * 1024) as f64);
    (gb >= 0.01).then(|| format!("{gb:.2} GB"))
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
        let content = format_display_content(None, "Written:", 0, 0.0, Some(0), 0, None);
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
        let content = format_display_content(
            Some(&progress),
            "Written:",
            500_000_000,
            100.0,
            Some(5),
            10,
            None,
        );
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
        let content_a = format_display_content(None, "Written:", 0, 0.0, Some(0), 0, None);
        let content_b = format_display_content(
            None,
            "Written:",
            999_999_999_999,
            999.99,
            Some(100),
            200,
            None,
        );
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
        let content_a =
            format_display_content(Some(&progress_a), "Written:", 0, 0.0, Some(0), 0, None);
        let content_b = format_display_content(
            Some(&progress_b),
            "Written:",
            999_999_999_999,
            999.99,
            Some(100),
            200,
            None,
        );
        let newlines_a = content_a.chars().filter(|&c| c == '\n').count();
        let newlines_b = content_b.chars().filter(|&c| c == '\n').count();
        assert_eq!(
            newlines_a, newlines_b,
            "Newline count must be the same for all data values in progress mode"
        );
    }

    #[test]
    fn content_without_seed_buffer_only_leaves_out_that_line() {
        let progress = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        for progress in [None, Some(&progress)] {
            let with_seed_buffer =
                format_display_content(progress, "Written:", 500_000_000, 100.0, Some(5), 10, None);
            let without_seed_buffer =
                format_display_content(progress, "Written:", 500_000_000, 100.0, None, 10, None);

            let seed_buffer_line = with_seed_buffer
                .lines()
                .find(|line| line.contains("Seed buffer:"))
                .expect("There should be a line for the seed buffer");
            assert_eq!(
                with_seed_buffer.replace(&format!("{seed_buffer_line}\n"), ""),
                without_seed_buffer
            );
        }
    }

    #[test]
    fn content_calls_the_processed_bytes_by_their_label() {
        let progress = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        for progress in [None, Some(&progress)] {
            let written =
                format_display_content(progress, "Written:", 500_000_000, 100.0, None, 10, None);
            let verified =
                format_display_content(progress, "Verified:", 500_000_000, 100.0, None, 10, None);

            assert!(written.contains("Written:"));
            assert!(!written.contains("Verified:"));
            // Labels are aligned to the right, so only the label and its padding differ
            assert_eq!(written.replace(" Written:", "Verified:"), verified);
        }
    }

    /// The line of the content that says how much is mismatched
    fn mismatched_line(mismatches: &MismatchesSoFar) -> String {
        let content = format_display_content(
            None,
            "Verified:",
            500_000_000,
            100.0,
            None,
            10,
            Some(mismatches),
        );
        let line = content
            .lines()
            .find(|line| line.contains("Mismatched:"))
            .expect("There should be a line for what is mismatched");
        line.to_string()
    }

    #[test]
    fn content_without_mismatches_only_leaves_out_that_line() {
        let progress = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        let mismatches = MismatchesSoFar::default();
        for progress in [None, Some(&progress)] {
            let with_mismatches = format_display_content(
                progress,
                "Verified:",
                500_000_000,
                100.0,
                None,
                10,
                Some(&mismatches),
            );
            let without_mismatches =
                format_display_content(progress, "Verified:", 500_000_000, 100.0, None, 10, None);

            assert!(!without_mismatches.contains("Mismatched:"));
            let mismatched_line = with_mismatches
                .lines()
                .find(|line| line.contains("Mismatched:"))
                .expect("There should be a line for what is mismatched");
            assert_eq!(
                with_mismatches.replace(&format!("{mismatched_line}\n"), ""),
                without_mismatches
            );
            // It follows the line that says how much was looked at
            let lines: Vec<&str> = with_mismatches.lines().collect();
            let line_number = lines.iter().position(|line| *line == mismatched_line);
            assert!(lines[line_number.unwrap() - 1].contains("Verified:"));
        }
    }

    #[test]
    fn content_says_that_nothing_is_mismatched() {
        let line = mismatched_line(&MismatchesSoFar::default());

        assert!(line.contains("0 bytes"), "{line}");
        assert!(!line.contains("part"), "{line}");
        assert!(!line.contains("first"), "{line}");
    }

    #[test]
    fn content_says_how_much_is_mismatched_and_where_that_begins() {
        for (num_bytes_mismatched, num_mismatches, first_mismatch_position, expected) in [
            (
                4096,
                1,
                8192,
                "4096 bytes in 1 part, the first at byte 8192",
            ),
            (20_480, 5, 0, "20480 bytes in 5 parts, the first at byte 0"),
            // What a device does that has less capacity than it claims to have
            (
                1_965_487_325_184,
                1,
                34_359_738_368,
                "1830.50 GB in 1 part, the first at 32.00 GB",
            ),
            // Right below what is shown in GB, and what is 0.01 GB
            (
                10_737_418,
                3,
                10_737_418,
                "10737418 bytes in 3 parts, the first at byte 10737418",
            ),
            (
                10_737_419,
                3,
                10_737_419,
                "0.01 GB in 3 parts, the first at 0.01 GB",
            ),
        ] {
            let line = mismatched_line(&MismatchesSoFar {
                num_bytes_mismatched,
                num_mismatches,
                first_mismatch_position: Some(first_mismatch_position),
            });

            assert!(line.contains(expected), "{line}");
        }
    }

    #[test]
    fn content_has_consistent_newlines_across_mismatches() {
        let progress = ProgressInfo {
            device_size: 1_000_000_000,
            skip_bytes: 0,
        };
        let no_mismatches = MismatchesSoFar::default();
        let many_mismatches = MismatchesSoFar {
            num_bytes_mismatched: 19_999_999_999_999,
            num_mismatches: 12_345_678,
            first_mismatch_position: Some(19_999_999_999_999),
        };
        for progress in [None, Some(&progress)] {
            let num_newlines = |mismatches| {
                format_display_content(progress, "Verified:", 0, 0.0, None, 0, Some(mismatches))
                    .chars()
                    .filter(|&c| c == '\n')
                    .count()
            };

            // The line count is what the cursor is moved up by, so the first mismatch must not
            // change it
            assert_eq!(num_newlines(&no_mismatches), num_newlines(&many_mismatches));
        }
    }

    #[test]
    fn mismatched_line_fits_into_a_terminal() {
        // A line that gets wrapped would count as two for the terminal, but as one for us when
        // we move the cursor up. This is as long as it gets, and 80 columns are the least that
        // a terminal usually has.
        let line = mismatched_line(&MismatchesSoFar {
            num_bytes_mismatched: 10_737_418,
            num_mismatches: 99_999_999,
            first_mismatch_position: Some(10_737_418),
        });

        let num_columns = line.replace(CLEAR_LINE, "").chars().count();
        assert!(num_columns < 80, "{num_columns} columns: {line}");
    }

    #[test]
    fn duration_is_formatted_as_hours_minutes_and_seconds() {
        assert_eq!("00:00:00", format_duration(0.0));
        assert_eq!("00:00:59", format_duration(59.9));
        assert_eq!("00:01:00", format_duration(60.0));
        assert_eq!("01:01:01", format_duration(3661.0));
        // More than a day stays in hours
        assert_eq!("100:00:00", format_duration(360_000.0));
    }

    #[test]
    fn duration_that_isnt_known_is_formatted_as_dashes() {
        assert_eq!("--:--:--", format_duration(f64::INFINITY));
        assert_eq!("--:--:--", format_duration(f64::NAN));
        assert_eq!("--:--:--", format_duration(-1.0));
    }

    #[test]
    fn content_shows_progress_and_time_left() {
        let progress = ProgressInfo {
            device_size: 4 * 1024 * 1024 * 1024,
            skip_bytes: 1024 * 1024 * 1024,
        };
        // 1 GiB was skipped and 1 GiB was written, so half of the device is done. The other
        // 2 GiB take 2048 seconds with 1 MiB per second.
        let content = format_display_content(
            Some(&progress),
            "Written:",
            1024 * 1024 * 1024,
            1.0,
            Some(5),
            10,
            None,
        );

        assert!(content.contains("50.0%"), "{content}");
        assert!(content.contains("2.00 / 4.00 GB"), "{content}");
        assert!(content.contains("00:34:08"), "{content}");
    }

    #[test]
    fn content_shows_no_time_left_without_speed() {
        let progress = ProgressInfo {
            device_size: 4 * 1024 * 1024 * 1024,
            skip_bytes: 0,
        };
        let content =
            format_display_content(Some(&progress), "Written:", 0, 0.0, Some(5), 10, None);

        assert!(content.contains("0.0%"), "{content}");
        assert!(content.contains("--:--:--"), "{content}");
    }

    #[test]
    fn content_doesnt_show_more_than_all_of_the_device() {
        // The device took more than its size, or its size is unknown
        for device_size in [1024 * 1024 * 1024, 0] {
            let progress = ProgressInfo {
                device_size,
                skip_bytes: 0,
            };
            let content = format_display_content(
                Some(&progress),
                "Written:",
                2 * 1024 * 1024 * 1024,
                100.0,
                Some(5),
                10,
                None,
            );

            assert!(content.contains("100.0%"), "{content}");
            assert!(content.contains("00:00:00"), "{content}");
        }
    }

    /// For a [Monitor] of something that doesn't compare data
    const NO_MISMATCHES: Option<fn() -> MismatchesSoFar> = None;

    /// A buffer with a fixed number of blocks in it, for a [Monitor] to show
    struct FakeBuffer(usize);

    impl<T> ProductReceiver<T> for FakeBuffer {
        fn blocking_get_product(&self) -> Result<T> {
            unimplemented!()
        }

        fn get_all_available_products(&self) -> Vec<T> {
            unimplemented!()
        }

        fn num_products_in_buffer(&self) -> usize {
            self.0
        }
    }

    #[test]
    fn display_asks_for_the_processed_bytes_each_time() {
        let num_calls = std::cell::Cell::new(0);
        let mut monitor = Monitor::new(
            Some(FakeBuffer(5)),
            FakeBuffer(10),
            "Written:",
            || {
                num_calls.set(num_calls.get() + 1);
                num_calls.get() * 1024 * 1024
            },
            NO_MISMATCHES,
            Some(ProgressInfo {
                device_size: 1024 * 1024 * 1024,
                skip_bytes: 0,
            }),
        );

        // What it prints goes to the terminal, so all we can see is that it works, also when
        // it overwrites what it printed before
        monitor.display().unwrap();
        monitor.display().unwrap();
        monitor.display().unwrap();

        assert_eq!(3, num_calls.get());
    }

    #[test]
    fn display_asks_for_the_mismatches_each_time() {
        let num_calls = std::cell::Cell::new(0);
        let seed_monitor: Option<FakeBuffer> = None;
        let mut monitor = Monitor::new(
            seed_monitor,
            FakeBuffer(10),
            "Verified:",
            || 0,
            Some(|| {
                num_calls.set(num_calls.get() + 1);
                MismatchesSoFar {
                    num_bytes_mismatched: num_calls.get() * 4096,
                    num_mismatches: 1,
                    first_mismatch_position: Some(0),
                }
            }),
            None,
        );

        monitor.display().unwrap();
        monitor.display().unwrap();
        monitor.display().unwrap();

        assert_eq!(3, num_calls.get());
    }

    #[test]
    fn display_works_without_seed_buffer_and_progress() {
        let seed_monitor: Option<FakeBuffer> = None;
        let mut monitor = Monitor::new(
            seed_monitor,
            FakeBuffer(10),
            "Verified:",
            || 0,
            NO_MISMATCHES,
            None,
        );

        monitor.display().unwrap();
        monitor.display().unwrap();
    }
}
