#![feature(write_all_vectored)]
#![feature(generic_associated_types)]

use anyhow::Result;
use running_average::RealTimeRunningAverage;
use std::fs::File;
use std::io::{self, Write};
use std::time::Duration;

mod block_writer;
mod byte_stream;
mod byte_stream_producer;
mod cancellation_token;
mod producer;
mod random;

use block_writer::BlockWriter;
use producer::Producer;

const SEED_GENERATOR_BLOCK_SIZE: usize = 256;
const NUM_SEED_BUFFER_BLOCKS: usize = 256;
const NUM_SEED_WORKERS: usize = 1;

const RANDOM_GENERATOR_BLOCK_SIZE: usize = 10 * 1024 * 1024;
const NUM_RANDOM_BUFFER_BLOCKS: usize = 100;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let seed_producer = byte_stream_producer::new_byte_stream_thread_pool_producer(
        SEED_GENERATOR_BLOCK_SIZE,
        NUM_SEED_BUFFER_BLOCKS,
        NUM_SEED_WORKERS,
        random::secure_seed_rng,
    )?;

    let num_random_workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2);

    let random_producer = byte_stream_producer::new_byte_stream_thread_pool_producer(
        RANDOM_GENERATOR_BLOCK_SIZE,
        NUM_RANDOM_BUFFER_BLOCKS,
        num_random_workers,
        || {
            random::secure_rng(byte_stream::byte_stream_from_producer(
                seed_producer.make_receiver(),
            ))
        },
    )?;

    let file = File::create("/home/heinzi/testfile")?;
    let writer = BlockWriter::new(random_producer.make_receiver(), file);

    let mut written_bytes = 0;
    let mut speed_calculator = RealTimeRunningAverage::default();
    while !writer.is_finished() {
        let new_written_bytes = writer.num_bytes_written();
        speed_calculator.insert((new_written_bytes - written_bytes) as f64);
        written_bytes = new_written_bytes;

        let written_gb = (new_written_bytes as f64) / ((1024 * 1024 * 1024) as f64);
        let current_speed_mb_s = speed_calculator.measurement().rate() / ((1024 * 1024) as f64);
        let num_seed_blocks = seed_producer.num_products_in_buffer();
        let num_random_blocks = random_producer.num_products_in_buffer();
        print!("\rWritten: {written_gb:.2} GB\tSpeed: {current_speed_mb_s:.2} MB/s\tSeedbuffer: {num_seed_blocks:.2}\tRandombuffer: {num_random_blocks:.2}");
        io::stdout().flush()?;

        std::thread::sleep(Duration::from_secs(1));
    }

    println!("Finished");

    writer.join();

    Ok(())
}
