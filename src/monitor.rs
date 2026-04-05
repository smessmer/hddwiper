use anyhow::Result;
use running_average::RealTimeRunningAverage;
use std::io::{self, Write};

use crate::block_writer::BlockWriter;
use crate::producer::ProductReceiver;

const CLEAR_LINE: &str = "\x1b[2K";
const NUM_DISPLAY_LINES: usize = 4;

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

        print!(
            "{CLEAR_LINE}  Written:        {written_gb:>10.2} GB\n\
             {CLEAR_LINE}  Speed:          {current_speed_mb_s:>10.2} MB/s\n\
             {CLEAR_LINE}  Seed buffer:    {num_seed_blocks:>10} blocks\n\
             {CLEAR_LINE}  Random buffer:  {num_random_blocks:>10} blocks",
        );
        io::stdout().flush()?;

        Ok(())
    }
}
