#![feature(write_all_vectored)]

use anyhow::{ensure, Context, Result};
use clap::Parser;
use std::fmt::Write as FmtWrite;
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::Deref;
use std::process::ExitCode;
use std::time::{Duration, Instant};

mod aligned_buffer;
mod aligned_reader;
mod aligned_writer;
mod block_verifier;
mod block_writer;
mod byte_stream;
mod monitor;
mod producer;
mod random;

use aligned_buffer::{AlignedBuffer, ALIGNMENT};
use aligned_reader::AlignedReader;
use aligned_writer::AlignedWriter;
use block_verifier::{BlockVerifier, VerificationResult};
use block_writer::BlockWriter;
use monitor::{Monitor, ProgressInfo};
use producer::{Producer, ProductReceiver, ThreadPoolProducer, ThreadPoolProductReceiver};
use random::Seed;

const SEED_GENERATOR_BLOCK_SIZE: usize = 256;
const NUM_SEED_BUFFER_BLOCKS: usize = 256;
const NUM_SEED_WORKERS: usize = 1;

/// A verification reads this many bytes from the device at a time. A wipe writes its blocks one
/// at a time, but collects blocks that are smaller than this to writes of this many bytes.
const DIRECT_IO_SIZE: usize = 4 * 1024 * 1024;

/// The display of what a wipe or a verification is doing gets updated this often
const DISPLAY_INTERVAL: Duration = Duration::from_secs(1);

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
    /// (example: --blocksize=10.2M).
    /// The size is rounded up to a multiple of 4096 bytes. Blocks that are smaller than 4M
    /// aren't written one by one, but are collected and written 4M at a time.
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

    /// Write random data that can be generated again later. For this, hddwiper derives all
    /// the data it writes from a seed and from the position on the device. This makes it
    /// possible to check later whether the device really stored what was written to it,
    /// see --verify. Without a value, hddwiper generates a random seed. Either way, it shows
    /// the seed before it starts to write, together with the command that verifies the wipe.
    /// To continue a deterministic wipe that was interrupted, pass the seed of that wipe
    /// with --deterministic=<SEED>, together with --skip-bytes.
    /// RDRAND isn't used in this mode.
    #[clap(long, value_name = "SEED", require_equals = true)]
    deterministic: Option<Option<Seed>>,

    /// Don't write anything, but check whether the device contains the data that a wipe with
    /// --deterministic wrote to it. The seed is the one that this wipe showed when it
    /// started. hddwiper reads the device from --skip-bytes to its end, compares it with the
    /// data that belongs to the seed, and lists the parts that differ. A device that doesn't
    /// return the data didn't store it, for example because it is defective or because it
    /// has less capacity than it claims to have.
    /// On Linux, the data is read from the device itself and not from the cache of the
    /// system, which can still have data that the device lost.
    #[clap(long, value_name = "SEED", conflicts_with = "deterministic")]
    verify: Option<Seed>,

    /// The file or device to write the random bytes to, or to verify
    output_file: String,
}

fn main() -> Result<ExitCode> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let args = Args::parse();

    match &args.verify {
        Some(seed) => verify(&args, seed),
        None => {
            wipe(&args)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Writes random data to the output file until it is full
fn wipe(args: &Args) -> Result<()> {
    let block_size = block_size(args)?;
    let skip_bytes = parse_num_bytes(&args.skip_bytes)?;

    // A deterministic wipe derives all its data from one seed, so it doesn't produce seeds
    let seed_producer = if args.deterministic.is_some() {
        None
    } else {
        if args.disable_rdrand {
            log::warn!("RDRAND generator is disabled. Generated bytes might be less random.");
        }
        Some(byte_stream::new_byte_stream_thread_pool_producer(
            SEED_GENERATOR_BLOCK_SIZE,
            |size| vec![0; size],
            NUM_SEED_BUFFER_BLOCKS,
            NUM_SEED_WORKERS,
            random::secure_seed_rng,
        )?)
    };

    let random_producer = match &seed_producer {
        Some(seed_producer) => byte_stream::new_byte_stream_thread_pool_producer(
            block_size,
            AlignedBuffer::new,
            args.buffersize as usize,
            num_random_workers(),
            || {
                Ok(random::secure_rng(
                    byte_stream::byte_stream_from_producer(seed_producer.make_receiver()),
                    args.disable_rdrand,
                ))
            },
        )?,
        None => {
            let given_seed = args.deterministic.clone().flatten();
            let continues_earlier_wipe = given_seed.is_some() && skip_bytes > 0;
            let seed = match given_seed {
                Some(seed) => seed,
                None => Seed::generate()?,
            };
            println!("Seed: {seed}");
            println!("To verify later that the device really stored the data of this run, use:");
            println!("  {}", verify_command(std::env::args(), &seed));
            if continues_earlier_wipe {
                println!(
                    "The data before --skip-bytes isn't part of this run. To verify it as well, \
                     use the command that the wipe you are continuing showed."
                );
            }
            deterministic_block_producer(
                &seed,
                block_size,
                args.buffersize as usize,
                num_random_workers(),
                first_block_position(skip_bytes, block_size),
            )?
        }
    };
    let random_monitor = random_producer.make_receiver();

    // A wipe that begins within a sector has to read the beginning of that sector
    let mut file = open_for_wipe(
        &args.output_file,
        AlignedWriter::<File>::has_to_read(skip_bytes),
    )?;

    // Detect total device size by seeking to end. This works for block devices
    // and partitions but returns 0 for newly-created regular files.
    let device_size = file.seek(SeekFrom::End(0))?;

    // The writer takes the file, but we need it once more when the writer is done
    let file_after_wipe = file.try_clone()?;
    let writer = start_writing(
        random_producer.make_receiver(),
        file,
        skip_bytes,
        block_size,
        DIRECT_IO_SIZE,
    )?;

    // Only show progress when we know the device size (block devices).
    // For regular files, device_size is 0 because opening them for a wipe truncates them.
    let progress_info = if device_size > skip_bytes {
        Some(ProgressInfo {
            device_size,
            skip_bytes,
        })
    } else {
        None
    };

    let mut monitor = Monitor::new(
        seed_producer
            .as_ref()
            .map(|seed_producer| seed_producer.make_receiver()),
        random_monitor,
        "Written:",
        || writer.num_bytes_written(),
        progress_info,
    );

    while !writer.is_finished() {
        monitor.display()?;

        wait_unless_finished(DISPLAY_INTERVAL, || writer.is_finished());
    }

    writer.join();

    println!("\nWaiting until all data is on the device...");
    sync(&file_after_wipe).context("Not all data could be written to the device")?;

    let end_position = (&file_after_wipe).stream_position()?;
    ensure_wipe_reached_end(&args.output_file, end_position, device_size)?;

    println!("Finished.");

    Ok(())
}

/// Opens the file to wipe. If possible, it is opened for direct writes, which go to the device
/// itself and not into the cache of the system. The cache is of no use for data that nobody
/// reads again, it only costs time. And with it, we would only see how much data the system
/// accepted and not how much the device did, and wouldn't get the error of a write that failed.
/// Direct writes have to be aligned, see [AlignedWriter].
fn open_for_wipe(path: &str, with_read_access: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create(true)
        .truncate(true)
        .read(with_read_access);

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;

        match options.clone().custom_flags(libc::O_DIRECT).open(path) {
            // This is how the system says that `path` can't be written directly
            Err(err) if err.kind() == ErrorKind::InvalidInput => {
                log::debug!("Writing {path} through the cache of the system: {err}");
            }
            result => return result,
        }
    }

    options.open(path)
}

/// The blocks of a wipe are for the parts of the device that begin at multiples of the block
/// size. This returns where the part begins that `start_position` is in. That is the part that
/// the first block of a wipe from `start_position` on has to be for.
///
/// Blocks consist of whole sectors, see [block_size]. So this way, each of them also begins at a
/// sector of the device, no matter where the wipe begins. Only such a block can be written to a
/// device directly from its own memory, see [AlignedWriter].
fn first_block_position(start_position: u64, block_size: usize) -> u64 {
    start_position - start_position % block_size as u64
}

/// Starts to write the blocks from `block_source` to `device`, from `position` on. The blocks
/// have `block_size` bytes, and the first one is for [first_block_position], so the part of it
/// that is in front of `position` isn't written.
///
/// The device gets aligned writes, see [AlignedWriter]. Blocks that are smaller than
/// `write_size` bytes are collected to writes of that size.
fn start_writing<B: 'static + Deref<Target = [u8]>>(
    block_source: impl 'static + ProductReceiver<B> + Send,
    device: impl 'static + Read + Write + Seek + Send,
    position: u64,
    block_size: usize,
    write_size: usize,
) -> io::Result<BlockWriter> {
    let writer = AlignedWriter::new(device, position, write_size)?;
    let num_bytes_to_skip = position - first_block_position(position, block_size);
    Ok(BlockWriter::new(
        block_source,
        writer,
        num_bytes_to_skip as usize,
    ))
}

/// A wipe ends when the device says that it is full. `end_position` is where that happened.
/// A device can also say so before its end, because it has less space than it claims to have.
/// Then the wipe isn't complete, and this fails.
fn ensure_wipe_reached_end(path: &str, end_position: u64, device_size: u64) -> Result<()> {
    ensure!(
        end_position >= device_size,
        "The wipe of {path} ended after {end_position} bytes, although its size is \
         {device_size} bytes"
    );
    Ok(())
}

/// Waits until all data that was written to `file` is on the device. Until then, the system can
/// still be holding back data that it accepted from us, and it can fail to write that data
/// without us ever knowing.
fn sync(file: &File) -> io::Result<()> {
    let result = file.sync_all();

    // On macOS, sync_all() also makes the drive write out its own cache, and some drives and
    // file systems can't do that. Then we at least wait until the system wrote everything.
    #[cfg(target_vendor = "apple")]
    let result = or_if_unsupported(result, || sync_without_drive_cache(file));

    ignore_if_unsupported(result)
}

/// Returns `sync_result`, unless it says that the file can't be synced. Then this tries
/// `other_sync` and returns its result.
#[cfg(any(target_vendor = "apple", test))]
fn or_if_unsupported(
    sync_result: io::Result<()>,
    other_sync: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    match sync_result {
        Err(err) if says_sync_is_unsupported(&err) => {
            log::debug!(
                "Syncing in another way, because the output file doesn't support it: {err}"
            );
            other_sync()
        }
        result => result,
    }
}

/// Waits until the system wrote all data of `file` to its drive. Other than [File::sync_all]
/// on macOS, this doesn't make the drive write out its own cache.
#[cfg(any(target_vendor = "apple", all(unix, test)))]
fn sync_without_drive_cache(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    loop {
        // SAFETY: fsync only needs a file descriptor that is open, and `file` keeps it open
        if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() != ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

/// Not everything can be synced. `/dev/null` can't, and on macOS some drives and file systems
/// can't either. That is no reason to fail. We then just don't know more than without syncing,
/// which is that the system accepted all the data.
fn ignore_if_unsupported(sync_result: io::Result<()>) -> io::Result<()> {
    match sync_result {
        Err(err) if says_sync_is_unsupported(&err) => {
            log::debug!("Not syncing, because the output file doesn't support it: {err}");
            Ok(())
        }
        result => result,
    }
}

/// Whether `err` says that the file can't be synced at all, and not that syncing it failed
fn says_sync_is_unsupported(err: &io::Error) -> bool {
    // This is what macOS answers, and there is no ErrorKind for it
    #[cfg(unix)]
    if err.raw_os_error() == Some(libc::ENOTTY) {
        return true;
    }
    matches!(err.kind(), ErrorKind::InvalidInput | ErrorKind::Unsupported)
}

/// Checks whether the output file contains the data of a deterministic wipe with `seed`.
/// The exit code says whether it does.
fn verify(args: &Args, seed: &Seed) -> Result<ExitCode> {
    let block_size = block_size(args)?;
    let skip_bytes = parse_num_bytes(&args.skip_bytes)?;

    let mut file = open_for_verification(&args.output_file)?;
    let device_size = file.seek(SeekFrom::End(0))?;
    ensure!(
        skip_bytes < device_size,
        "There is nothing to verify, because --skip-bytes isn't smaller than the size of {}, \
         which is {device_size} bytes",
        args.output_file
    );
    let reader = AlignedReader::new(file, skip_bytes, DIRECT_IO_SIZE)?;

    let random_producer = deterministic_block_producer(
        seed,
        block_size,
        args.buffersize as usize,
        num_random_workers(),
        skip_bytes,
    )?;
    let verifier = BlockVerifier::new(random_producer.make_receiver(), reader, skip_bytes);

    // Verification doesn't produce seeds, so there is no seed buffer to show
    let seed_monitor: Option<ThreadPoolProductReceiver<Vec<u8>>> = None;
    let mut monitor = Monitor::new(
        seed_monitor,
        random_producer.make_receiver(),
        "Verified:",
        || verifier.num_bytes_verified(),
        Some(ProgressInfo {
            device_size,
            skip_bytes,
        }),
    );

    while !verifier.is_finished() {
        monitor.display()?;

        wait_unless_finished(DISPLAY_INTERVAL, || verifier.is_finished());
    }

    let result = verifier.join()?;
    // Otherwise we would call a device fine that didn't return all of its data
    ensure!(
        result.num_bytes_verified >= device_size - skip_bytes,
        "{} ended after {} bytes, although its size is {device_size} bytes",
        args.output_file,
        skip_bytes + result.num_bytes_verified
    );
    println!("\n{}", format_verification_result(&result));

    Ok(if result.num_bytes_mismatched == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Opens the file to verify. If possible, it is opened for direct reads, which get their data
/// from the device itself and not from what the system has cached of it. Otherwise, a device
/// that lost data could pass the verification because the system still knows what was written.
/// Direct reads have to be aligned, see [AlignedReader].
fn open_for_verification(path: &str) -> io::Result<File> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let direct_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(path);
        match direct_file {
            // This is how the system says that the file system of `path` has no direct reads
            Err(err) if err.kind() == ErrorKind::InvalidInput => {
                log::warn!(
                    "{path} can't be read past the cache of the system. For data that is still \
                     in that cache, the verification can't tell whether it was really stored."
                );
            }
            result => return result,
        }
    }

    File::open(path)
}

/// Waits for `duration`, but not longer than it takes until `is_finished` returns true. So a run
/// ends when its work is done, and not only when the display is updated the next time.
fn wait_unless_finished(duration: Duration, is_finished: impl Fn() -> bool) {
    const CHECK_INTERVAL: Duration = Duration::from_millis(10);

    let start = Instant::now();
    while !is_finished() {
        let remaining = duration.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(CHECK_INTERVAL));
    }
}

fn block_size(args: &Args) -> Result<usize> {
    let block_size = parse_num_bytes(&args.blocksize)? as usize;
    // Blocks without data would never fill the device, nor reach its end
    ensure!(block_size > 0, "--blocksize has to be larger than zero");
    // Blocks have to consist of whole sectors, see [first_block_position]. Nothing else depends
    // on their exact size.
    block_size
        .checked_next_multiple_of(ALIGNMENT)
        .context("--blocksize is too large")
}

fn num_random_workers() -> usize {
    std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2)
}

/// Produces the blocks of a deterministic wipe with `seed`, beginning with the block for
/// `start_position` on the device. Each position of the device gets the data that the random
/// stream of the seed has at the same position. A wipe and its verification both take their
/// blocks from here, which is what makes them agree on the data.
fn deterministic_block_producer(
    seed: &Seed,
    block_size: usize,
    buffer_num_blocks: usize,
    num_workers: usize,
    start_position: u64,
) -> Result<ThreadPoolProducer<AlignedBuffer>> {
    byte_stream::new_seekable_byte_stream_thread_pool_producer(
        block_size,
        AlignedBuffer::new,
        buffer_num_blocks,
        num_workers,
        start_position,
        || Ok(random::deterministic_rng(seed)),
    )
}

/// What we tell the user when a verification is finished
fn format_verification_result(result: &VerificationResult) -> String {
    const GB: f64 = (1024 * 1024 * 1024) as f64;
    let verified_gb = result.num_bytes_verified as f64 / GB;

    if result.num_bytes_mismatched == 0 {
        return format!(
            "Verification successful: All {} bytes ({verified_gb:.2} GB) are the data that was \
             written.",
            result.num_bytes_verified
        );
    }

    let mut message = format!(
        "Verification FAILED: {} of {} bytes ({:.2} of {verified_gb:.2} GB) are in sectors that \
         don't contain the data that was written.\n\
         These parts of the device differ (first to last byte):",
        result.num_bytes_mismatched,
        result.num_bytes_verified,
        result.num_bytes_mismatched as f64 / GB,
    );
    for mismatch in &result.first_mismatches {
        write!(message, "\n  {} - {}", mismatch.start, mismatch.end - 1).unwrap();
    }
    let num_not_listed = result.num_mismatches - result.first_mismatches.len() as u64;
    if num_not_listed > 0 {
        write!(message, "\n  ... and {num_not_listed} more").unwrap();
    }
    message
}

/// The arguments of the command line that verifies a deterministic wipe. They are the arguments
/// the wipe was started with, but with `--verify <seed>` instead of `--deterministic` or
/// `--deterministic=<seed>`, so that the verification looks at the same device with the same
/// options.
fn verify_args(wipe_args: impl Iterator<Item = String>, seed: &Seed) -> Vec<String> {
    let mut replaced_deterministic = false;
    wipe_args
        .flat_map(|arg| {
            let is_deterministic = arg == "--deterministic" || arg.starts_with("--deterministic=");
            if is_deterministic && !replaced_deterministic {
                replaced_deterministic = true;
                vec!["--verify".to_string(), seed.to_string()]
            } else {
                vec![arg]
            }
        })
        .collect()
}

/// The command line of [verify_args], in a form that can be pasted into a shell
fn verify_command(wipe_args: impl Iterator<Item = String>, seed: &Seed) -> String {
    let verify_args: Vec<String> = verify_args(wipe_args, seed)
        .iter()
        .map(|arg| shell_quote(arg))
        .collect();
    verify_args.join(" ")
}

/// Quotes `arg` so that a POSIX shell takes it as one argument with exactly this content
fn shell_quote(arg: &str) -> String {
    let needs_no_quotes = |c: char| c.is_ascii_alphanumeric() || "_-+=:,./@%".contains(c);
    if !arg.is_empty() && arg.chars().all(needs_no_quotes) {
        arg.to_string()
    } else {
        // Within single quotes, everything but a single quote is taken literally
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
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
    // The lists of mismatches we expect often have exactly one range, which clippy takes for a mistake
    #![allow(clippy::single_range_in_vec_init)]

    use super::*;
    use crate::byte_stream::{SeekableByteStream, SyncByteStream};
    use std::io::Cursor;
    use std::ops::Range;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_deterministic_arg() {
        let args = Args::try_parse_from(["hddwiper", "/dev/null"]).unwrap();
        assert_eq!(None, args.deterministic);

        // Without a seed. What follows is the output file, not a seed.
        let args = Args::try_parse_from(["hddwiper", "--deterministic", "/dev/null"]).unwrap();
        assert_eq!(Some(None), args.deterministic);
        assert_eq!("/dev/null", args.output_file);

        let args = Args::try_parse_from(["hddwiper", "/dev/null", "--deterministic"]).unwrap();
        assert_eq!(Some(None), args.deterministic);
        assert_eq!("/dev/null", args.output_file);
    }

    #[test]
    fn test_deterministic_arg_with_seed() {
        let seed = random::Seed::generate().unwrap();

        let args = Args::try_parse_from([
            "hddwiper",
            &format!("--deterministic={seed}"),
            "--skip-bytes=5G",
            "/dev/null",
        ])
        .unwrap();
        assert_eq!(Some(Some(seed.clone())), args.deterministic);
        assert_eq!("/dev/null", args.output_file);

        // A seed has to be valid
        assert!(Args::try_parse_from(["hddwiper", "--deterministic=1234", "/dev/null"]).is_err());
        assert!(Args::try_parse_from(["hddwiper", "--deterministic=", "/dev/null"]).is_err());

        // Without the equals sign, the seed would be taken for the output file. This must fail
        // instead of wiping a file with the name of the seed.
        assert!(Args::try_parse_from([
            "hddwiper",
            "--deterministic",
            &seed.to_string(),
            "/dev/null"
        ])
        .is_err());
    }

    fn verify_command_for(wipe_command: &[&str], seed: &random::Seed) -> String {
        verify_command(wipe_command.iter().map(|arg| arg.to_string()), seed)
    }

    #[test]
    fn test_verify_command() {
        let seed = random::Seed::generate().unwrap();

        assert_eq!(
            format!("hddwiper --verify {seed} /dev/sdb"),
            verify_command_for(&["hddwiper", "--deterministic", "/dev/sdb"], &seed)
        );

        // Everything else stays as it is, in the same order
        assert_eq!(
            format!("./hddwiper -b 1M --skip-bytes=5G --verify {seed} -u 3 /dev/sdb"),
            verify_command_for(
                &[
                    "./hddwiper",
                    "-b",
                    "1M",
                    "--skip-bytes=5G",
                    "--deterministic",
                    "-u",
                    "3",
                    "/dev/sdb"
                ],
                &seed
            )
        );

        // A seed that was passed to the wipe is the one to verify with
        assert_eq!(
            format!("hddwiper --verify {seed} --skip-bytes 5G /dev/sdb"),
            verify_command_for(
                &[
                    "hddwiper",
                    &format!("--deterministic={seed}"),
                    "--skip-bytes",
                    "5G",
                    "/dev/sdb"
                ],
                &seed
            )
        );

        // Only the option is replaced, not a file that happens to have its name
        assert_eq!(
            format!("hddwiper --verify {seed} -- --deterministic"),
            verify_command_for(
                &["hddwiper", "--deterministic", "--", "--deterministic"],
                &seed
            )
        );
    }

    #[test]
    fn test_verify_command_quotes_arguments() {
        let seed = random::Seed::generate().unwrap();

        assert_eq!(
            format!(r"'/my tools/hddwiper' --verify {seed} '/mnt/it'\''s a file'"),
            verify_command_for(
                &["/my tools/hddwiper", "--deterministic", "/mnt/it's a file"],
                &seed
            )
        );
    }

    #[test]
    fn test_verify_arg() {
        let seed = Seed::generate().unwrap();

        let args = Args::try_parse_from(["hddwiper", "/dev/null"]).unwrap();
        assert_eq!(None, args.verify);

        let args =
            Args::try_parse_from(["hddwiper", "--verify", &seed.to_string(), "/dev/null"]).unwrap();
        assert_eq!(Some(seed.clone()), args.verify);
        assert_eq!("/dev/null", args.output_file);

        let args =
            Args::try_parse_from(["hddwiper", &format!("--verify={seed}"), "/dev/null"]).unwrap();
        assert_eq!(Some(seed.clone()), args.verify);

        // It needs a valid seed
        assert!(Args::try_parse_from(["hddwiper", "--verify", "/dev/null"]).is_err());
        assert!(Args::try_parse_from(["hddwiper", "--verify", "1234", "/dev/null"]).is_err());

        // Writing and verifying at the same time isn't possible
        assert!(Args::try_parse_from([
            "hddwiper",
            "--deterministic",
            "--verify",
            &seed.to_string(),
            "/dev/null"
        ])
        .is_err());
    }

    #[test]
    fn test_verify_command_is_understood() {
        let seed = Seed::generate().unwrap();
        let wipe_commands = [
            vec![
                "hddwiper".to_string(),
                "--deterministic".to_string(),
                "/dev/sdb".to_string(),
            ],
            vec![
                "./hddwiper".to_string(),
                "--blocksize=1M".to_string(),
                format!("--deterministic={seed}"),
                "-s".to_string(),
                "5G".to_string(),
                "--disable-rdrand".to_string(),
                "-u".to_string(),
                "3".to_string(),
                "/mnt/it's a file".to_string(),
            ],
        ];

        for wipe_command in wipe_commands {
            let wipe_args = Args::try_parse_from(&wipe_command).unwrap();
            let verify_args =
                Args::try_parse_from(verify_args(wipe_command.into_iter(), &seed)).unwrap();

            // It verifies the same device with the same options
            assert_eq!(Some(seed.clone()), verify_args.verify);
            assert_eq!(None, verify_args.deterministic);
            assert_eq!(wipe_args.output_file, verify_args.output_file);
            assert_eq!(wipe_args.skip_bytes, verify_args.skip_bytes);
            assert_eq!(wipe_args.blocksize, verify_args.blocksize);
            assert_eq!(wipe_args.buffersize, verify_args.buffersize);
        }
    }

    #[test]
    fn test_sync() {
        let path = std::env::temp_dir().join(format!("hddwiper-test-sync-{}", std::process::id()));
        let mut file = File::create(&path).unwrap();
        file.write_all(b"some data").unwrap();

        let result = sync(&file);

        std::fs::remove_file(&path).unwrap();
        result.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_sync_of_file_that_cant_be_synced() {
        let file = File::create("/dev/null").unwrap();
        // That is why sync() has to ignore it
        assert_eq!(ErrorKind::InvalidInput, file.sync_all().unwrap_err().kind());

        sync(&file).unwrap();
    }

    #[test]
    fn test_or_if_unsupported() {
        let unreachable = || -> io::Result<()> { panic!("The other sync must not be tried") };

        // A sync that worked or failed is the result
        assert!(or_if_unsupported(Ok(()), unreachable).is_ok());
        let storage_full = or_if_unsupported(Err(ErrorKind::StorageFull.into()), unreachable);
        assert_eq!(ErrorKind::StorageFull, storage_full.unwrap_err().kind());
        let failure = or_if_unsupported(Err(io::Error::other("Simulated failure")), unreachable);
        assert_eq!("Simulated failure", failure.unwrap_err().to_string());

        // A sync that isn't supported is replaced by the other one, whatever that one returns
        for unsupported in [ErrorKind::InvalidInput, ErrorKind::Unsupported] {
            assert!(or_if_unsupported(Err(unsupported.into()), || Ok(())).is_ok());
            let failure = or_if_unsupported(Err(unsupported.into()), || {
                Err(io::Error::other("Simulated failure"))
            });
            assert_eq!("Simulated failure", failure.unwrap_err().to_string());
            // Also if the other one isn't supported either. What to do then isn't decided here.
            let still_unsupported = or_if_unsupported(Err(unsupported.into()), || {
                Err(ErrorKind::Unsupported.into())
            });
            assert_eq!(
                ErrorKind::Unsupported,
                still_unsupported.unwrap_err().kind()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_or_if_unsupported_with_the_error_of_macos() {
        // What macOS says when a drive or a file system can't do its kind of sync
        let unsupported = || Err(io::Error::from_raw_os_error(libc::ENOTTY));

        assert!(or_if_unsupported(unsupported(), || Ok(())).is_ok());
        let failure = or_if_unsupported(unsupported(), || {
            Err(io::Error::from_raw_os_error(libc::EIO))
        });
        assert_eq!(Some(libc::EIO), failure.unwrap_err().raw_os_error());
    }

    #[cfg(unix)]
    #[test]
    fn test_sync_without_drive_cache() {
        let path = std::env::temp_dir().join(format!(
            "hddwiper-test-sync-without-drive-cache-{}",
            std::process::id()
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(b"some data").unwrap();

        let result = sync_without_drive_cache(&file);

        std::fs::remove_file(&path).unwrap();
        result.unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_sync_without_drive_cache_of_file_that_cant_be_synced() {
        let file = File::create("/dev/null").unwrap();

        // It returns the error of the system, which sync() then ignores
        let err = sync_without_drive_cache(&file).unwrap_err();
        assert_eq!(ErrorKind::InvalidInput, err.kind());
        assert!(ignore_if_unsupported(Err(err)).is_ok());
    }

    #[test]
    fn test_ignore_if_unsupported() {
        assert!(ignore_if_unsupported(Ok(())).is_ok());
        assert!(ignore_if_unsupported(Err(ErrorKind::InvalidInput.into())).is_ok());
        assert!(ignore_if_unsupported(Err(ErrorKind::Unsupported.into())).is_ok());

        // Everything else means that data could be lost
        let storage_full = ignore_if_unsupported(Err(ErrorKind::StorageFull.into()));
        assert_eq!(ErrorKind::StorageFull, storage_full.unwrap_err().kind());
        let failure = ignore_if_unsupported(Err(io::Error::other("Simulated failure")));
        assert_eq!("Simulated failure", failure.unwrap_err().to_string());
    }

    #[cfg(unix)]
    #[test]
    fn test_ignore_if_unsupported_with_errors_of_the_system() {
        // The ways in which systems say that a file can't be synced
        for errno in [
            libc::EINVAL,
            libc::ENOSYS,
            libc::ENOTSUP,
            libc::EOPNOTSUPP,
            libc::ENOTTY,
        ] {
            let err = io::Error::from_raw_os_error(errno);
            assert!(
                ignore_if_unsupported(Err(err)).is_ok(),
                "Error {errno} isn't ignored"
            );
        }

        // And some of the ways in which they say that data was lost
        for errno in [
            libc::EIO,
            libc::ENOSPC,
            libc::EDQUOT,
            libc::EROFS,
            libc::ENODEV,
        ] {
            let err = io::Error::from_raw_os_error(errno);
            assert_eq!(
                Some(errno),
                ignore_if_unsupported(Err(err)).unwrap_err().raw_os_error(),
                "Error {errno} is ignored"
            );
        }
    }

    /// A file for a test. It is removed when this is dropped.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(name: &str) -> Self {
            let file_name = format!("hddwiper-test-{name}-{}", std::process::id());
            Self(std::env::temp_dir().join(file_name))
        }

        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn test_data(num_bytes: usize) -> Vec<u8> {
        (0..num_bytes).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn test_open_for_wipe() {
        const WRITE_SIZE: usize = 16 * aligned_buffer::ALIGNMENT;
        let temp_file = TempFile::new("open-for-wipe");
        // It has content already, which a wipe replaces
        std::fs::write(temp_file.path(), test_data(1_000_000)).unwrap();
        // Whole sectors, because where the file is opened for direct writes, nothing else can
        // be flushed
        let data = test_data(50 * aligned_buffer::ALIGNMENT);

        // Where the file is opened for direct writes, this only works if the writes are aligned
        let file = open_for_wipe(temp_file.path(), false).unwrap();
        let mut writer = AlignedWriter::new(file, 0, WRITE_SIZE).unwrap();
        writer.write_all(&data).unwrap();
        writer.flush().unwrap();

        assert!(data == std::fs::read(temp_file.path()).unwrap());
    }

    #[test]
    fn test_open_for_wipe_that_begins_within_a_sector() {
        const WRITE_SIZE: usize = 16 * aligned_buffer::ALIGNMENT;
        const POSITION: usize = 5000;
        let temp_file = TempFile::new("open-for-wipe-within-sector");
        // So that position and data together are whole sectors
        let data = test_data(50 * aligned_buffer::ALIGNMENT - POSITION);

        // For this, the writer has to read the file
        assert!(AlignedWriter::<File>::has_to_read(POSITION as u64));
        let file = open_for_wipe(temp_file.path(), true).unwrap();
        let mut writer = AlignedWriter::new(file, POSITION as u64, WRITE_SIZE).unwrap();
        writer.write_all(&data).unwrap();
        writer.flush().unwrap();

        // The file is new, so in front of the position there is nothing but zeroes
        let content = std::fs::read(temp_file.path()).unwrap();
        assert!(content[..POSITION].iter().all(|byte| *byte == 0));
        assert!(data == content[POSITION..]);
    }

    #[cfg(unix)]
    #[test]
    fn test_open_for_wipe_without_direct_writes() {
        // On Linux, /dev/null can't be opened for direct writes. It has to be opened anyway.
        let mut file = open_for_wipe("/dev/null", false).unwrap();
        file.write_all(b"some data").unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_wipe_of_full_device() {
        // /dev/full never has space for anything, so this is a wipe that ends at once
        let file = open_for_wipe("/dev/full", false).unwrap();
        let producer = deterministic_block_producer(&Seed::generate().unwrap(), 1000, 3, 2, 0);
        let writer = start_writing(
            producer.unwrap().make_receiver(),
            file,
            0,
            1000,
            DIRECT_IO_SIZE,
        )
        .unwrap();

        // The writer ends without failing, and that is all we can see of it
        writer.join();
    }

    #[test]
    fn test_ensure_wipe_reached_end() {
        // A new file has no size that a wipe could miss
        assert!(ensure_wipe_reached_end("file", 0, 0).is_ok());
        assert!(ensure_wipe_reached_end("file", 123_456, 0).is_ok());
        // A device is full at its end
        assert!(ensure_wipe_reached_end("/dev/sdb", 1_000_000, 1_000_000).is_ok());

        // But not before
        assert_eq!(
            "The wipe of /dev/sdb ended after 16384 bytes, although its size is 65536 bytes",
            ensure_wipe_reached_end("/dev/sdb", 16_384, 65_536)
                .unwrap_err()
                .to_string()
        );
        assert!(ensure_wipe_reached_end("/dev/sdb", 999_999, 1_000_000).is_err());
    }

    #[test]
    fn test_open_for_verification() {
        let content: Vec<u8> = (0..300_000).map(|i| (i % 251) as u8).collect();
        let path = std::env::temp_dir().join(format!("hddwiper-test-open-{}", std::process::id()));
        std::fs::write(&path, &content).unwrap();

        // Where the file is opened for direct reads, this only works if the reads are aligned
        let file = open_for_verification(path.to_str().unwrap()).unwrap();
        let mut reader = AlignedReader::new(file, 4321, 16 * aligned_buffer::ALIGNMENT).unwrap();
        let mut read_content = vec![];
        let result = reader.read_to_end(&mut read_content);

        std::fs::remove_file(&path).unwrap();
        result.unwrap();
        assert!(content[4321..] == read_content);
    }

    #[cfg(unix)]
    #[test]
    fn test_open_for_verification_without_direct_reads() {
        // On Linux, /dev/null can't be opened for direct reads. It has to be opened anyway.
        let mut file = open_for_verification("/dev/null").unwrap();
        let mut content = vec![];
        file.read_to_end(&mut content).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_open_for_verification_fails_for_missing_file() {
        let path = std::env::temp_dir().join(format!("hddwiper-test-none-{}", std::process::id()));
        let err = open_for_verification(path.to_str().unwrap()).unwrap_err();
        assert_eq!(ErrorKind::NotFound, err.kind());
    }

    #[test]
    fn test_wait_unless_finished_waits_for_the_duration() {
        let start = Instant::now();

        wait_unless_finished(Duration::from_millis(50), || false);

        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn test_wait_unless_finished_ends_when_finished() {
        // Finished from the beginning
        let start = Instant::now();
        wait_unless_finished(Duration::from_secs(1000), || true);
        assert!(start.elapsed() < Duration::from_secs(100));

        // Finished after a while
        let start = Instant::now();
        wait_unless_finished(Duration::from_secs(1000), || {
            start.elapsed() >= Duration::from_millis(30)
        });
        assert!(start.elapsed() >= Duration::from_millis(30));
        assert!(start.elapsed() < Duration::from_secs(100));
    }

    fn block_size_for(blocksize_arg: &str) -> Result<usize> {
        let blocksize_arg = format!("--blocksize={blocksize_arg}");
        block_size(&Args::try_parse_from(["hddwiper", &blocksize_arg, "/dev/null"]).unwrap())
    }

    #[test]
    fn test_blocksize_must_not_be_zero() {
        assert!(block_size_for("0").is_err());
        assert!(block_size_for("1").is_ok());
    }

    #[test]
    fn test_blocksize_is_rounded_up_to_whole_sectors() {
        assert_eq!(4096, block_size_for("1").unwrap());
        assert_eq!(4096, block_size_for("4095").unwrap());
        assert_eq!(4096, block_size_for("4096").unwrap());
        assert_eq!(8192, block_size_for("4097").unwrap());
        assert_eq!(1024 * 1024, block_size_for("1M").unwrap());
        assert_eq!(10 * 1024 * 1024, block_size_for("10M").unwrap());
        // The example of the help text, which is 10695475.2 bytes
        assert_eq!(10_698_752, block_size_for("10.2M").unwrap());
        assert_eq!(0, block_size_for("10.2M").unwrap() % 4096);
    }

    #[test]
    fn test_blocksize_must_not_be_too_large_to_round_it() {
        assert_eq!(
            "--blocksize is too large",
            block_size_for("99999999999T").unwrap_err().to_string()
        );
    }

    #[test]
    fn test_default_blocksize_is_written_without_collecting_it() {
        let args = Args::try_parse_from(["hddwiper", "/dev/null"]).unwrap();
        let block_size = block_size(&args).unwrap();

        // Otherwise the blocks would be copied on their way to the device
        assert_eq!(0, block_size % aligned_buffer::ALIGNMENT);
        assert!(block_size >= DIRECT_IO_SIZE);
    }

    #[test]
    fn test_first_block_position() {
        const BLOCK_SIZE: usize = 8192;

        assert_eq!(0, first_block_position(0, BLOCK_SIZE));
        assert_eq!(0, first_block_position(1, BLOCK_SIZE));
        assert_eq!(0, first_block_position(8191, BLOCK_SIZE));
        assert_eq!(8192, first_block_position(8192, BLOCK_SIZE));
        assert_eq!(8192, first_block_position(8193, BLOCK_SIZE));
        assert_eq!(8192, first_block_position(12_288, BLOCK_SIZE));
        assert_eq!(81_920, first_block_position(90_000, BLOCK_SIZE));
        // Far behind what 32 bit can count
        assert_eq!(
            (1 << 45) + 8192,
            first_block_position((1 << 45) + 10_000, BLOCK_SIZE)
        );
    }

    #[test]
    fn test_format_verification_result() {
        assert_eq!(
            "Verification successful: All 3221225472 bytes (3.00 GB) are the data that was written.",
            format_verification_result(&VerificationResult {
                num_bytes_verified: 3 * 1024 * 1024 * 1024,
                num_bytes_mismatched: 0,
                num_mismatches: 0,
                first_mismatches: vec![],
            })
        );

        assert_eq!(
            "Verification FAILED: 2147487744 of 3221225472 bytes (2.00 of 3.00 GB) are in sectors \
             that don't contain the data that was written.\n\
             These parts of the device differ (first to last byte):\n  \
             4096 - 8191\n  \
             1073741824 - 3221225471",
            format_verification_result(&VerificationResult {
                num_bytes_verified: 3 * 1024 * 1024 * 1024,
                num_bytes_mismatched: 2 * 1024 * 1024 * 1024 + 4096,
                num_mismatches: 2,
                first_mismatches: vec![4096..8192, 1024 * 1024 * 1024..3 * 1024 * 1024 * 1024],
            })
        );

        // Not all mismatches are listed
        assert_eq!(
            "Verification FAILED: 20480 of 3221225472 bytes (0.00 of 3.00 GB) are in sectors \
             that don't contain the data that was written.\n\
             These parts of the device differ (first to last byte):\n  \
             4096 - 8191\n  \
             ... and 4 more",
            format_verification_result(&VerificationResult {
                num_bytes_verified: 3 * 1024 * 1024 * 1024,
                num_bytes_mismatched: 5 * 4096,
                num_mismatches: 5,
                first_mismatches: vec![4096..8192],
            })
        );
    }

    /// How a [MemoryDevice] treats what is written to it
    #[derive(Clone, Copy)]
    enum DeviceKind {
        /// It stores everything
        Honest,
        /// It only has storage for `real_size` bytes and starts over at its beginning when
        /// more is written to it. That is what many devices with a faked capacity do.
        WrapsAround { real_size: usize },
        /// It only has storage for `real_size` bytes and ignores what is written beyond that
        DropsWrites { real_size: usize },
    }

    /// A device in memory that claims to have a size of `claimed_size` bytes
    #[derive(Clone)]
    struct MemoryDevice {
        kind: DeviceKind,
        claimed_size: usize,
        storage: Arc<Mutex<Vec<u8>>>,

        /// Whether it only accepts aligned reads and writes, like a device that was opened for
        /// direct reads and writes
        needs_alignment: bool,

        /// The positions that each write wrote to
        writes: Arc<Mutex<Vec<Range<usize>>>>,
    }

    impl MemoryDevice {
        fn new(kind: DeviceKind, claimed_size: usize) -> Self {
            let real_size = match kind {
                DeviceKind::Honest => claimed_size,
                DeviceKind::WrapsAround { real_size } | DeviceKind::DropsWrites { real_size } => {
                    real_size
                }
            };
            Self {
                kind,
                claimed_size,
                storage: Arc::new(Mutex::new(vec![0; real_size])),
                needs_alignment: false,
                writes: Arc::new(Mutex::new(vec![])),
            }
        }

        /// A device that stores everything, but only accepts aligned reads and writes
        fn that_needs_alignment(size: usize) -> Self {
            assert_eq!(0, size % aligned_buffer::ALIGNMENT);
            Self {
                needs_alignment: true,
                ..Self::new(DeviceKind::Honest, size)
            }
        }

        /// Where the byte at `position` of the device is in the storage, if it is stored at all
        fn storage_position(&self, position: usize) -> Option<usize> {
            match self.kind {
                DeviceKind::Honest => Some(position),
                DeviceKind::WrapsAround { real_size } => Some(position % real_size),
                DeviceKind::DropsWrites { real_size } => (position < real_size).then_some(position),
            }
        }

        /// The device as something that can be read and written like a file
        fn file(&self) -> MemoryDeviceFile {
            MemoryDeviceFile {
                device: self.clone(),
                position: 0,
            }
        }

        /// What reading the whole device returns
        fn content(&self) -> Vec<u8> {
            let storage = self.storage.lock().unwrap();
            (0..self.claimed_size)
                .map(|position| {
                    self.storage_position(position)
                        .map_or(0, |storage_position| storage[storage_position])
                })
                .collect()
        }
    }

    struct MemoryDeviceFile {
        device: MemoryDevice,
        position: usize,
    }

    impl MemoryDeviceFile {
        fn assert_is_aligned_if_needed(&self, data: &[u8]) {
            if self.device.needs_alignment {
                assert_eq!(0, self.position % aligned_buffer::ALIGNMENT);
                assert_eq!(0, data.len() % aligned_buffer::ALIGNMENT);
                assert_eq!(0, data.as_ptr() as usize % aligned_buffer::ALIGNMENT);
            }
        }
    }

    impl Read for MemoryDeviceFile {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            self.assert_is_aligned_if_needed(dest);
            let content = self.device.content();
            let remaining = &content[self.position.min(content.len())..];
            let len = dest.len().min(remaining.len());
            dest[..len].copy_from_slice(&remaining[..len]);
            self.position += len;
            Ok(len)
        }
    }

    impl Seek for MemoryDeviceFile {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            match position {
                SeekFrom::Start(position) => {
                    self.position = position as usize;
                    Ok(position)
                }
                _ => Err(ErrorKind::Unsupported.into()),
            }
        }
    }

    impl Write for MemoryDeviceFile {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.assert_is_aligned_if_needed(data);
            if self.position >= self.device.claimed_size {
                return Err(io::Error::new(ErrorKind::StorageFull, "Device is full"));
            }
            let len = data.len().min(self.device.claimed_size - self.position);
            let mut storage = self.device.storage.lock().unwrap();
            for (offset, byte) in data[..len].iter().enumerate() {
                if let Some(storage_position) = self.device.storage_position(self.position + offset)
                {
                    storage[storage_position] = *byte;
                }
            }
            let written = self.position..self.position + len;
            self.device.writes.lock().unwrap().push(written);
            self.position += len;
            Ok(len)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Does what a deterministic wipe does, with `device` instead of a file
    fn wipe_deterministically(
        device: &MemoryDevice,
        seed: &Seed,
        block_size: usize,
        num_workers: usize,
        skip_bytes: u64,
    ) {
        // The devices are small, so that only small writes make it many writes
        const WRITE_SIZE: usize = 2 * aligned_buffer::ALIGNMENT;

        let producer = deterministic_block_producer(
            seed,
            block_size,
            3,
            num_workers,
            first_block_position(skip_bytes, block_size),
        )
        .unwrap();
        let writer = start_writing(
            producer.make_receiver(),
            device.file(),
            skip_bytes,
            block_size,
            WRITE_SIZE,
        )
        .unwrap();
        while !writer.is_finished() {
            std::thread::sleep(Duration::from_millis(1));
        }
        writer.join();
    }

    /// Does what a verification does, with `device` instead of a file
    fn verify_device(
        device: &MemoryDevice,
        seed: &Seed,
        block_size: usize,
        num_workers: usize,
        skip_bytes: u64,
    ) -> VerificationResult {
        let producer =
            deterministic_block_producer(seed, block_size, 3, num_workers, skip_bytes).unwrap();
        let mut reader = Cursor::new(device.content());
        reader.set_position(skip_bytes);
        BlockVerifier::new(producer.make_receiver(), reader, skip_bytes)
            .join()
            .unwrap()
    }

    /// The random stream of `seed` from position `skip_bytes` on, and zeroes before that
    fn data_of_seed(seed: &Seed, skip_bytes: usize, size: usize) -> Vec<u8> {
        let mut data = vec![0; size];
        let mut rng = random::deterministic_rng(seed);
        rng.seek(skip_bytes as u64).unwrap();
        rng.blocking_read(&mut data[skip_bytes..]).unwrap();
        data
    }

    #[test]
    fn test_deterministic_wipe_writes_data_of_seed() {
        // Not a multiple of any of the block sizes, so the last block doesn't fit
        const DEVICE_SIZE: usize = 100_003;
        let seed = Seed::generate().unwrap();

        // Small blocks, so that many of them are written and their order matters
        for (block_size, num_workers, skip_bytes) in [
            (1000, 1, 0),
            (1000, 4, 0),
            (4096, 7, 0),
            (33_333, 3, 0),
            (200_000, 2, 0),
            (1000, 4, 12_345),
            // Blocks of whole sectors that are large enough to be written as they are, in
            // wipes that begin with a block, within a block and within a sector
            (8192, 4, 0),
            (16_384, 3, 0),
            (16_384, 3, 32_768),
            (16_384, 3, 20_480),
            (8192, 4, 12_345),
            (65_536, 2, 70_000),
        ] {
            let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);

            wipe_deterministically(&device, &seed, block_size, num_workers, skip_bytes);

            assert!(
                data_of_seed(&seed, skip_bytes as usize, DEVICE_SIZE) == device.content(),
                "Wrong data with block size {block_size}, {num_workers} workers and {skip_bytes} \
                 skipped bytes"
            );
        }
    }

    #[test]
    fn test_wipe_keeps_data_before_skip_bytes() {
        const DEVICE_SIZE: usize = 100_003;
        let first_seed = Seed::generate().unwrap();
        let second_seed = Seed::generate().unwrap();

        // At the beginning of a sector, and within one. The beginning of that one is written
        // again, and has to stay as it is.
        for skip_bytes in [8192, 12_345, 99_999] {
            // Blocks that are collected, and blocks that are written as they are
            for block_size in [1000, 16_384] {
                let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);
                wipe_deterministically(&device, &first_seed, 1000, 4, 0);

                wipe_deterministically(&device, &second_seed, block_size, 4, skip_bytes as u64);

                let mut expected = data_of_seed(&first_seed, 0, DEVICE_SIZE);
                expected[skip_bytes..].copy_from_slice(
                    &data_of_seed(&second_seed, skip_bytes, DEVICE_SIZE)[skip_bytes..],
                );
                assert!(
                    expected == device.content(),
                    "Wrong data after a wipe that skipped {skip_bytes} bytes, with blocks of \
                     {block_size} bytes"
                );
            }
        }
    }

    #[test]
    fn test_wipe_writes_blocks_of_whole_sectors_as_they_are() {
        // Larger than the writes that smaller blocks are collected to in these tests
        const BLOCK_SIZE: usize = 4 * aligned_buffer::ALIGNMENT;
        // It ends within a block
        const DEVICE_SIZE: usize = 10 * BLOCK_SIZE + 2 * aligned_buffer::ALIGNMENT;
        let seed = Seed::generate().unwrap();

        // Wipes that begin with a block, within a block at a sector, and within a sector
        for skip_bytes in [
            0,
            3 * BLOCK_SIZE,
            aligned_buffer::ALIGNMENT,
            2 * BLOCK_SIZE + 3 * aligned_buffer::ALIGNMENT,
            12_345,
            5 * BLOCK_SIZE + 1,
            6 * BLOCK_SIZE - 1,
        ] {
            let device = MemoryDevice::that_needs_alignment(DEVICE_SIZE);

            wipe_deterministically(&device, &seed, BLOCK_SIZE, 4, skip_bytes as u64);

            assert!(
                data_of_seed(&seed, skip_bytes, DEVICE_SIZE) == device.content(),
                "Wrong data after a wipe that skipped {skip_bytes} bytes"
            );
            // Behind the first block, each write is for one whole block, and the last write
            // for as much of its block as the device has space for. Writes that collected the
            // data wouldn't have this size.
            let second_block_position =
                first_block_position(skip_bytes as u64, BLOCK_SIZE) as usize + BLOCK_SIZE;
            let expected_writes: Vec<Range<usize>> = (second_block_position..DEVICE_SIZE)
                .step_by(BLOCK_SIZE)
                .map(|position| position..(position + BLOCK_SIZE).min(DEVICE_SIZE))
                .collect();
            let writes = device.writes.lock().unwrap();
            let writes_behind_first_block: Vec<Range<usize>> = writes
                .iter()
                .filter(|write| write.start >= second_block_position)
                .cloned()
                .collect();
            assert_eq!(
                expected_writes, writes_behind_first_block,
                "Wrong writes after skipping {skip_bytes} bytes"
            );
        }
    }

    #[test]
    fn test_verify_succeeds_for_wiped_device() {
        const DEVICE_SIZE: usize = 100_003;
        let seed = Seed::generate().unwrap();
        let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);

        wipe_deterministically(&device, &seed, 1000, 4, 0);

        // It doesn't have to use the block size and the number of workers of the wipe
        for (block_size, num_workers) in [(1000, 4), (4096, 1), (77_777, 3)] {
            assert_eq!(
                VerificationResult {
                    num_bytes_verified: DEVICE_SIZE as u64,
                    num_bytes_mismatched: 0,
                    num_mismatches: 0,
                    first_mismatches: vec![],
                },
                verify_device(&device, &seed, block_size, num_workers, 0)
            );
        }
    }

    #[test]
    fn test_verify_with_skip_bytes() {
        const DEVICE_SIZE: usize = 100_003;
        const SKIP_BYTES: u64 = 12_345;
        let seed = Seed::generate().unwrap();
        let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);

        wipe_deterministically(&device, &seed, 1000, 4, SKIP_BYTES);

        // What was written is fine
        assert_eq!(
            VerificationResult {
                num_bytes_verified: DEVICE_SIZE as u64 - SKIP_BYTES,
                num_bytes_mismatched: 0,
                num_mismatches: 0,
                first_mismatches: vec![],
            },
            verify_device(&device, &seed, 1000, 4, SKIP_BYTES)
        );

        // What was skipped isn't. The mismatch includes the whole sector the wipe started in.
        assert_eq!(
            VerificationResult {
                num_bytes_verified: DEVICE_SIZE as u64,
                num_bytes_mismatched: 16_384,
                num_mismatches: 1,
                first_mismatches: vec![0..16_384],
            },
            verify_device(&device, &seed, 1000, 4, 0)
        );
    }

    #[test]
    fn test_verify_fails_with_wrong_seed() {
        const DEVICE_SIZE: usize = 100_003;
        let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);

        wipe_deterministically(&device, &Seed::generate().unwrap(), 1000, 4, 0);

        assert_eq!(
            VerificationResult {
                num_bytes_verified: DEVICE_SIZE as u64,
                num_bytes_mismatched: DEVICE_SIZE as u64,
                num_mismatches: 1,
                first_mismatches: vec![0..DEVICE_SIZE as u64],
            },
            verify_device(&device, &Seed::generate().unwrap(), 1000, 4, 0)
        );
    }

    #[test]
    fn test_verify_finds_changed_byte() {
        const DEVICE_SIZE: usize = 100_003;
        let seed = Seed::generate().unwrap();
        let device = MemoryDevice::new(DeviceKind::Honest, DEVICE_SIZE);

        wipe_deterministically(&device, &seed, 1000, 4, 0);
        device.storage.lock().unwrap()[50_000] ^= 1;

        assert_eq!(
            VerificationResult {
                num_bytes_verified: DEVICE_SIZE as u64,
                num_bytes_mismatched: 4096,
                num_mismatches: 1,
                first_mismatches: vec![49_152..53_248],
            },
            verify_device(&device, &seed, 1000, 4, 0)
        );
    }

    #[test]
    fn test_verify_finds_device_that_wraps_around() {
        // It claims to have 64 KiB but only has 16 KiB
        let seed = Seed::generate().unwrap();
        let device = MemoryDevice::new(DeviceKind::WrapsAround { real_size: 16_384 }, 65_536);

        wipe_deterministically(&device, &seed, 1000, 4, 0);

        // Each write of a later part overwrote the earlier ones, so only the last 16 KiB are right
        assert_eq!(
            VerificationResult {
                num_bytes_verified: 65_536,
                num_bytes_mismatched: 49_152,
                num_mismatches: 1,
                first_mismatches: vec![0..49_152],
            },
            verify_device(&device, &seed, 1000, 4, 0)
        );
    }

    #[test]
    fn test_verify_finds_device_that_drops_writes() {
        // It claims to have 64 KiB but only has 16 KiB
        let seed = Seed::generate().unwrap();
        let device = MemoryDevice::new(DeviceKind::DropsWrites { real_size: 16_384 }, 65_536);

        wipe_deterministically(&device, &seed, 1000, 4, 0);

        // Only the first 16 KiB got stored
        assert_eq!(
            VerificationResult {
                num_bytes_verified: 65_536,
                num_bytes_mismatched: 49_152,
                num_mismatches: 1,
                first_mismatches: vec![16_384..65_536],
            },
            verify_device(&device, &seed, 1000, 4, 0)
        );
    }

    #[test]
    fn test_shell_quote() {
        assert_eq!("/dev/sdb1", shell_quote("/dev/sdb1"));
        assert_eq!("--skip-bytes=3.4K", shell_quote("--skip-bytes=3.4K"));
        assert_eq!("''", shell_quote(""));
        assert_eq!("'a b'", shell_quote("a b"));
        assert_eq!("'$HOME'", shell_quote("$HOME"));
        assert_eq!("'~/file'", shell_quote("~/file"));
        assert_eq!("'*'", shell_quote("*"));
        assert_eq!(r"'it'\''s'", shell_quote("it's"));
    }

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
