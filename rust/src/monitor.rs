use anyhow::Result;
use running_average::RealTimeRunningAverage;
use std::io::{self, Write};
use std::time::Duration;

use crate::block_writer::BlockWriter;
use crate::producer::ProductReceiver;

pub struct Monitor<'w, M1, M2, M3>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<Vec<u8>>,
    M3: ProductReceiver<Vec<u8>>,
{
    seed_monitor: M1,
    random_monitor_xsalsa: M2,
    random_monitor_rdrand: M3,
    writer: &'w BlockWriter,
    speed_calculator: RealTimeRunningAverage<f64>,
    written_bytes: u64,
}

impl<'w, M1, M2, M3> Monitor<'w, M1, M2, M3>
where
    M1: ProductReceiver<Vec<u8>>,
    M2: ProductReceiver<Vec<u8>>,
    M3: ProductReceiver<Vec<u8>>,
{
    pub fn new(
        seed_monitor: M1,
        random_monitor_xsalsa: M2,
        random_monitor_rdrand: M3,
        writer: &'w BlockWriter,
    ) -> Self {
        Self {
            seed_monitor,
            random_monitor_xsalsa,
            random_monitor_rdrand,
            writer,
            speed_calculator: RealTimeRunningAverage::default(),
            written_bytes: 0,
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
        let num_random_blocks_xsalsa = self.random_monitor_xsalsa.num_products_in_buffer();
        let num_random_blocks_rdrand = self.random_monitor_rdrand.num_products_in_buffer();
        print!("\rWritten: {written_gb:.2} GB\tSpeed: {current_speed_mb_s:.2} MB/s\tSeedbuffer: {num_seed_blocks:3}\tRandombuffer(XSalsa20): {num_random_blocks_xsalsa:3}\tRandombuffer(Rdrand): {num_random_blocks_rdrand:3}");
        io::stdout().flush()?;

        Ok(())
    }
}
