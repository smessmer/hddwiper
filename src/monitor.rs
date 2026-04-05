use anyhow::Result;
use colored::Colorize;
use running_average::RealTimeRunningAverage;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};

use crate::block_writer::BlockWriter;
use crate::producer::ProductReceiver;

const CLEAR_LINE: &str = "\x1b[2K";
const NUM_DISPLAY_LINES: usize = 5;

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
    has_displayed: bool,
}

impl<'w, M1, M2> Monitor<'w, M1, M2>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<Vec<u8>>,
{
    pub fn new(seed_monitor: M1, random_monitor: M2, writer: &'w BlockWriter) -> Self {
        Self {
            seed_monitor,
            random_monitor,
            writer,
            speed_calculator: RealTimeRunningAverage::default(),
            written_bytes: 0,
            has_displayed: false,
        }
    }

    pub fn display(&mut self) -> Result<()> {
        let new_written_bytes = self.writer.num_bytes_written();
        self.speed_calculator
            .insert((new_written_bytes - self.written_bytes) as f64);
        self.written_bytes = new_written_bytes;

        let written_gb = (new_written_bytes as f64) / ((1024 * 1024 * 1024) as f64);
        let current_speed_mb_s =
            self.speed_calculator.measurement().rate() / ((1024 * 1024) as f64);
        let num_seed_blocks = self.seed_monitor.num_products_in_buffer();
        let num_random_blocks = self.random_monitor.num_products_in_buffer();

        // Move cursor up to overwrite previous output
        if self.has_displayed {
            print!("\x1b[{}A", NUM_DISPLAY_LINES);
        }
        self.has_displayed = true;

        let mut written_val = String::new();
        write!(written_val, "{written_gb:.2} GB")?;
        let mut speed_val = String::new();
        write!(speed_val, "{current_speed_mb_s:.2} MB/s")?;

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
        io::stdout().flush()?;

        Ok(())
    }
}
