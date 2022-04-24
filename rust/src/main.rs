use anyhow::Result;

mod byte_stream;
mod byte_stream_producer;
mod cancellation_token;
mod producer;
mod random;

use producer::Producer;

const SEED_GENERATOR_BLOCK_SIZE: usize = 256;
const NUM_SEED_BUFFER_BLOCKS: usize = 256;
const NUM_SEED_WORKERS: usize = 1;

const RANDOM_GENERATOR_BLOCK_SIZE: usize = 100 * 1024 * 1024;
const NUM_RANDOM_BUFFER_BLOCKS: usize = 5;

#[tokio::main]
async fn main() -> Result<()> {
    let seed_producer = byte_stream_producer::new_byte_stream_thread_pool_producer(
        SEED_GENERATOR_BLOCK_SIZE,
        NUM_SEED_BUFFER_BLOCKS,
        NUM_SEED_WORKERS,
        || random::secure_seed_rng(),
    )?;

    let num_random_workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2);

    let random_producer = byte_stream_producer::new_byte_stream_thread_pool_producer(
        RANDOM_GENERATOR_BLOCK_SIZE,
        NUM_RANDOM_BUFFER_BLOCKS,
        num_random_workers,
        || random::secure_rng(byte_stream::byte_stream_from_producer(seed_producer.make_consumer())),
    )?;

    Ok(())
}
