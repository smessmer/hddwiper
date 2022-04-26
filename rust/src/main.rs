#![feature(write_all_vectored)]
#![feature(generic_associated_types)]
#![feature(io_error_more)]

use anyhow::Result;
use clap::Parser;
use std::fs::File;
use std::time::Duration;

mod block_writer;
mod byte_stream;
mod byte_stream_producer;
mod cancellation_token;
mod composite_xor_producer;
mod monitor;
mod producer;
mod random;

use block_writer::BlockWriter;
use composite_xor_producer::CompositeXorProducer;
use monitor::Monitor;
use producer::Producer;

const SEED_GENERATOR_BLOCK_SIZE: usize = 256;
const NUM_SEED_BUFFER_BLOCKS: usize = 256;
const NUM_SEED_WORKERS: usize = 1;

const RANDOM_GENERATOR_BLOCK_SIZE: usize = 10 * 1024 * 1024;
const NUM_RANDOM_BUFFER_BLOCKS: usize = 100;

/// Output a random stream of bytes to a hard drive or partition to wipe it.
/// This will continue wiping until the device runs out of space.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// How many bytes to skip at the beginning of the output device.
    /// This can be useful if a previous wipe was interrupted and you
    /// want to continue it.
    #[clap(short, long, default_value_t = 0)]
    skip_bytes: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    let seed_producer = byte_stream_producer::new_byte_stream_thread_pool_producer(
        SEED_GENERATOR_BLOCK_SIZE,
        NUM_SEED_BUFFER_BLOCKS,
        NUM_SEED_WORKERS,
        random::secure_seed_rng,
    )?;

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
    let monitor_xsalsa = random_producer_xsalsa.make_receiver();
    let random_producer_rdrand = byte_stream_producer::new_byte_stream_thread_pool_producer(
        RANDOM_GENERATOR_BLOCK_SIZE,
        NUM_RANDOM_BUFFER_BLOCKS,
        num_random_workers,
        || Ok(random::rng_rdrand_or_zeroes()),
    )?;
    let monitor_rdrand = random_producer_rdrand.make_receiver();

    let random_producer = CompositeXorProducer::new(random_producer_rdrand, random_producer_xsalsa);

    let file = File::create("/home/heinzi/testfile")?;
    let writer = BlockWriter::new(random_producer.make_receiver(), file);

    let mut monitor = Monitor::new(
        seed_producer.make_receiver(),
        monitor_xsalsa,
        monitor_rdrand,
        &writer,
    );

    while !writer.is_finished() {
        monitor.display()?;

        std::thread::sleep(Duration::from_secs(1));
    }

    println!("\nFinished");

    writer.join();

    Ok(())
}
