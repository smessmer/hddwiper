#![feature(write_all_vectored)]
#![feature(generic_associated_types)]
#![feature(io_error_more)]
#![feature(scoped_threads)]

use anyhow::{ensure, Result};
use clap::Parser;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::time::Duration;

mod block_writer;
mod byte_stream;
mod cancellation_token;
mod monitor;
mod producer;
mod random;

use block_writer::BlockWriter;
use monitor::Monitor;
use producer::Producer;

const SEED_GENERATOR_BLOCK_SIZE: usize = 256;
const NUM_SEED_BUFFER_BLOCKS: usize = 256;
const NUM_SEED_WORKERS: usize = 1;

/// Output a random stream of bytes to a hard drive or partition to wipe it.
/// This will continue wiping until the device runs out of space.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of bytes to skip at the start of the output file.
    /// This can be useful if a previous wipe was interrupted and you
    /// want to continue it.
    /// You can either give the amount in bytes or use one of the
    /// suffixes K,M,G,T, when you want to use the corresponding power
    /// of 1024.
    /// You can use floating point values (for example 3.4K).
    #[clap(short, long, default_value_t = String::from("0"))]
    skip_bytes: String,

    /// Size of the random blocks that are generated, stored in memory, and in one
    /// go written to the disk. For the allowed option syntax see the skip option
    /// (example: --blocksize=10.2M)
    /// Warning: With high blocksize, the amount of RAM required goes up. This can
    /// cause the program to be killed.
    #[clap(short, long, default_value_t = String::from("10M"))]
    blocksize: String,

    /// Maximum number of random blocks (see --blocksize for the size of the blocks)
    /// to produce beforehand and keep in memory (Hard disk is usually slower than the
    /// random generator).
    /// Warning: With high buffersize, the amount of RAM required goes up. This can
    /// cause the program to be killed.
    #[clap(short = 'u', long, default_value_t = 10)]
    buffersize: u64,

    /// Disable RDRAND generator. By default, if the CPU has RDRAND, then hddwiper uses
    /// both, a software random generator and RDRAND, and xors the streams together for
    /// better randomness. But on older CPUs, RDRAND is slow. With this option,
    /// hddwiper will only use the software generator.
    #[clap(long)]
    disable_rdrand: bool,

    /// The file or device to write the random bytes to
    output_file: String,
}

fn main() -> Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();

    let seed_producer = byte_stream::new_byte_stream_thread_pool_producer(
        SEED_GENERATOR_BLOCK_SIZE,
        NUM_SEED_BUFFER_BLOCKS,
        NUM_SEED_WORKERS,
        random::secure_seed_rng,
    )?;

    let num_random_workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2);

    let random_generator_block_size = parse_num_bytes(&args.blocksize)?;

    let random_producer = byte_stream::new_byte_stream_thread_pool_producer(
        random_generator_block_size as usize,
        args.buffersize as usize,
        num_random_workers,
        || {
            Ok(random::secure_rng(
                byte_stream::byte_stream_from_producer(seed_producer.make_receiver()),
                args.disable_rdrand,
            ))
        },
    )?;
    let random_monitor = random_producer.make_receiver();

    let mut file = File::create(args.output_file)?;
    file.seek(SeekFrom::Start(parse_num_bytes(&args.skip_bytes)?))?;
    let writer = BlockWriter::new(random_producer.make_receiver(), file);

    let mut monitor = Monitor::new(seed_producer.make_receiver(), random_monitor, &writer);

    while !writer.is_finished() {
        monitor.display()?;

        std::thread::sleep(Duration::from_secs(1));
    }

    println!("\nFinished");

    writer.join();

    Ok(())
}

fn parse_num_bytes(value: &str) -> Result<u64> {
    ensure!(
        !value.is_empty(),
        "Cannot parse empty string as a number of bytes"
    );

    const KB: f64 = 1024f64;
    const MB: f64 = 1024f64 * 1024f64;
    const GB: f64 = 1024f64 * 1024f64 * 1024f64;
    const TB: f64 = 1024f64 * 1024f64 * 1024f64 * 1024f64;

    Ok(match value.chars().last().unwrap() {
        'K' | 'k' => (value[..(value.len() - 1)].parse::<f64>()? * KB) as u64,
        'M' | 'm' => (value[..(value.len() - 1)].parse::<f64>()? * MB) as u64,
        'G' | 'g' => (value[..(value.len() - 1)].parse::<f64>()? * GB) as u64,
        'T' | 't' => (value[..(value.len() - 1)].parse::<f64>()? * TB) as u64,
        _ => value.parse::<u64>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_num_bytes() {
        assert!(parse_num_bytes("").is_err());
        assert_eq!(0, parse_num_bytes("0").unwrap());
        assert!(parse_num_bytes(".5").is_err());
        assert!(parse_num_bytes("0.5").is_err());
        assert_eq!(1, parse_num_bytes("1").unwrap());
        assert_eq!(500, parse_num_bytes("500").unwrap());
        assert!(parse_num_bytes("500.123").is_err());
        assert_eq!(100_000, parse_num_bytes("100000").unwrap());

        assert!(parse_num_bytes("k").is_err());
        assert_eq!(0, parse_num_bytes("0k").unwrap());
        assert_eq!(512, parse_num_bytes(".5k").unwrap());
        assert_eq!(512, parse_num_bytes("0.5k").unwrap());
        assert_eq!(1024, parse_num_bytes("1k").unwrap());
        assert_eq!(500 * 1024, parse_num_bytes("500k").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64) as u64,
            parse_num_bytes("500.123k").unwrap()
        );
        assert_eq!(100_000 * 1024, parse_num_bytes("100000k").unwrap());

        assert!(parse_num_bytes("K").is_err());
        assert_eq!(0, parse_num_bytes("0K").unwrap());
        assert_eq!(512, parse_num_bytes(".5K").unwrap());
        assert_eq!(512, parse_num_bytes("0.5K").unwrap());
        assert_eq!(1024, parse_num_bytes("1K").unwrap());
        assert_eq!(500 * 1024, parse_num_bytes("500K").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64) as u64,
            parse_num_bytes("500.123K").unwrap()
        );
        assert_eq!(100_000 * 1024, parse_num_bytes("100000K").unwrap());

        assert!(parse_num_bytes("m").is_err());
        assert_eq!(0, parse_num_bytes("0m").unwrap());
        assert_eq!(512 * 1024, parse_num_bytes(".5m").unwrap());
        assert_eq!(512 * 1024, parse_num_bytes("0.5m").unwrap());
        assert_eq!(1024 * 1024, parse_num_bytes("1m").unwrap());
        assert_eq!(500 * 1024 * 1024, parse_num_bytes("500m").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123m").unwrap()
        );
        assert_eq!(100_000 * 1024 * 1024, parse_num_bytes("100000m").unwrap());

        assert!(parse_num_bytes("M").is_err());
        assert_eq!(0, parse_num_bytes("0M").unwrap());
        assert_eq!(512 * 1024, parse_num_bytes(".5M").unwrap());
        assert_eq!(512 * 1024, parse_num_bytes("0.5M").unwrap());
        assert_eq!(1024 * 1024, parse_num_bytes("1M").unwrap());
        assert_eq!(500 * 1024 * 1024, parse_num_bytes("500M").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123M").unwrap()
        );
        assert_eq!(100_000 * 1024 * 1024, parse_num_bytes("100000M").unwrap());

        assert!(parse_num_bytes("g").is_err());
        assert_eq!(0, parse_num_bytes("0g").unwrap());
        assert_eq!(512 * 1024 * 1024, parse_num_bytes(".5g").unwrap());
        assert_eq!(512 * 1024 * 1024, parse_num_bytes("0.5g").unwrap());
        assert_eq!(1024 * 1024 * 1024, parse_num_bytes("1g").unwrap());
        assert_eq!(500 * 1024 * 1024 * 1024, parse_num_bytes("500g").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123g").unwrap()
        );
        assert_eq!(
            100_000 * 1024 * 1024 * 1024,
            parse_num_bytes("100000g").unwrap()
        );

        assert!(parse_num_bytes("G").is_err());
        assert_eq!(0, parse_num_bytes("0G").unwrap());
        assert_eq!(512 * 1024 * 1024, parse_num_bytes(".5G").unwrap());
        assert_eq!(512 * 1024 * 1024, parse_num_bytes("0.5G").unwrap());
        assert_eq!(1024 * 1024 * 1024, parse_num_bytes("1G").unwrap());
        assert_eq!(500 * 1024 * 1024 * 1024, parse_num_bytes("500G").unwrap());
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123G").unwrap()
        );
        assert_eq!(
            100_000 * 1024 * 1024 * 1024,
            parse_num_bytes("100000G").unwrap()
        );

        assert!(parse_num_bytes("t").is_err());
        assert_eq!(0, parse_num_bytes("0t").unwrap());
        assert_eq!(512 * 1024 * 1024 * 1024, parse_num_bytes(".5t").unwrap());
        assert_eq!(512 * 1024 * 1024 * 1024, parse_num_bytes("0.5t").unwrap());
        assert_eq!(1024 * 1024 * 1024 * 1024, parse_num_bytes("1t").unwrap());
        assert_eq!(
            500 * 1024 * 1024 * 1024 * 1024,
            parse_num_bytes("500t").unwrap()
        );
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123t").unwrap()
        );
        assert_eq!(
            100_000 * 1024 * 1024 * 1024 * 1024,
            parse_num_bytes("100000t").unwrap()
        );

        assert!(parse_num_bytes("T").is_err());
        assert_eq!(0, parse_num_bytes("0T").unwrap());
        assert_eq!(512 * 1024 * 1024 * 1024, parse_num_bytes(".5T").unwrap());
        assert_eq!(512 * 1024 * 1024 * 1024, parse_num_bytes("0.5T").unwrap());
        assert_eq!(1024 * 1024 * 1024 * 1024, parse_num_bytes("1T").unwrap());
        assert_eq!(
            500 * 1024 * 1024 * 1024 * 1024,
            parse_num_bytes("500T").unwrap()
        );
        assert_eq!(
            (500.123f64 * 1024f64 * 1024f64 * 1024f64 * 1024f64) as u64,
            parse_num_bytes("500.123T").unwrap()
        );
        assert_eq!(
            100_000 * 1024 * 1024 * 1024 * 1024,
            parse_num_bytes("100000T").unwrap()
        );

        assert!(parse_num_bytes("abc").is_err());
        assert!(parse_num_bytes("5c").is_err());
        assert!(parse_num_bytes("abc4").is_err());
        assert!(parse_num_bytes("a4bc4").is_err());
    }
}
