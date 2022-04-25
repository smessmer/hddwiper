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
mod composite_xor_producer;
mod producer;
mod random;

use block_writer::BlockWriter;
use composite_xor_producer::CompositeXorProducer;
use producer::{Producer, ProductReceiver};

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
    let seed_monitor = seed_producer.make_receiver();

    let num_random_workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2);

    let random_producer_xsalsa = byte_stream_producer::new_byte_stream_thread_pool_producer(
        RANDOM_GENERATOR_BLOCK_SIZE,
        NUM_RANDOM_BUFFER_BLOCKS,
        num_random_workers,
        || {
            Ok(random::rng_xsalsa(byte_stream::byte_stream_from_producer(
                seed_producer.make_receiver(),
            )))
        },
    )?;
    let random_monitor_xsalsa = random_producer_xsalsa.make_receiver();
    let random_producer_rdrand = byte_stream_producer::new_byte_stream_thread_pool_producer(
        RANDOM_GENERATOR_BLOCK_SIZE,
        NUM_RANDOM_BUFFER_BLOCKS,
        num_random_workers,
        || Ok(random::rng_rdrand_or_zeroes()),
    )?;
    let random_monitor_rdrand = random_producer_rdrand.make_receiver();

    let random_producer = CompositeXorProducer::new(random_producer_rdrand, random_producer_xsalsa);

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
        let num_seed_blocks = seed_monitor.num_products_in_buffer();
        let num_random_blocks_xsalsa = random_monitor_xsalsa.num_products_in_buffer();
        let num_random_blocks_rdrand = random_monitor_rdrand.num_products_in_buffer();
        print!("\rWritten: {written_gb:.2} GB\tSpeed: {current_speed_mb_s:.2} MB/s\tSeedbuffer: {num_seed_blocks:3}\tRandombuffer(XSalsa20): {num_random_blocks_xsalsa:3}\tRandombuffer(Rdrand): {num_random_blocks_rdrand:3}");
        io::stdout().flush()?;

        std::thread::sleep(Duration::from_secs(1));
    }

    println!("Finished");

    writer.join();

    Ok(())
}
