//! Tests that let the hddwiper binary wipe and verify real block devices and file systems.
//!
//! The block devices are loop devices, which keep their data in a file. Setting them up and
//! mounting file systems needs root, so these tests are ignored unless they are asked for:
//!
//! ```text
//! CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=sudo cargo test --release --test devices -- --ignored
//! ```
//!
//! That lets cargo build them as usual, and only run them with sudo. CI does the same.
#![cfg(target_os = "linux")]

mod common;

use common::{data_of_seed, hddwiper, hddwiper_with_exactly, shown_seed, Run};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const MIB: usize = 1024 * 1024;

/// The block sizes that the tests wipe with. A wipe collects small blocks to larger writes, and
/// writes large ones as they are. The last one is the size that blocks have by default.
const BLOCK_SIZES: [&str; 3] = ["64K", "4M", "10M"];

/// Runs a command that sets something up, and returns what it printed
fn run(program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("{program} can't be run: {err}"));
    assert!(
        output.status.success(),
        "{program} {args:?} failed. These tests need root. {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A directory for the files of one test. It is removed when this is dropped.
struct TestDir(PathBuf);

impl TestDir {
    /// Each test has to use another `name`
    fn new(name: &str) -> Self {
        let dir_name = format!("hddwiper-device-test-{name}-{}", std::process::id());
        let dir = std::env::temp_dir().join(dir_name);
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A mounted file system. It is unmounted when this is dropped.
struct FileSystem(PathBuf);

impl FileSystem {
    /// A file system in memory that only has space for `size` bytes
    fn in_memory(mount_point: PathBuf, size: usize) -> Self {
        Self::mount(
            mount_point,
            &["-t", "tmpfs", "-o", &format!("size={size}"), "tmpfs"],
        )
    }

    /// A new file system on `device`
    fn on_device(mount_point: PathBuf, device: &LoopDevice) -> Self {
        run("mkfs.ext4", &["-q", &device.path]);
        Self::mount(mount_point, &[&device.path])
    }

    fn mount(mount_point: PathBuf, what: &[&str]) -> Self {
        std::fs::create_dir(&mount_point).unwrap();
        let mut args = what.to_vec();
        args.push(mount_point.to_str().unwrap());
        run("mount", &args);
        Self(mount_point)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for FileSystem {
    fn drop(&mut self) {
        // This fails for as long as something still uses the file system
        let unmounted = wait_until(|| {
            let umount = Command::new("umount").arg(&self.0).output();
            umount.is_ok_and(|output| output.status.success())
        });
        if !unmounted {
            // Then it at least goes away when it isn't used anymore
            let _ = Command::new("umount").arg("-l").arg(&self.0).output();
        }
    }
}

/// A block device that keeps its data in a file. It is detached when this is dropped.
struct LoopDevice {
    path: String,
    backing_file: PathBuf,
}

impl LoopDevice {
    /// A device that claims to have `size` bytes. Whether it can store them depends on the
    /// space that the file system of `backing_file` has.
    fn new(backing_file: PathBuf, size: usize, sector_size: usize) -> Self {
        File::create(&backing_file)
            .unwrap()
            .set_len(size as u64)
            .unwrap();
        let path = run(
            "losetup",
            &[
                "--sector-size",
                &sector_size.to_string(),
                "--find",
                "--show",
                backing_file.to_str().unwrap(),
            ],
        );
        Self { path, backing_file }
    }

    /// What reading the device returns
    fn content(&self) -> Vec<u8> {
        std::fs::read(&self.path).unwrap()
    }

    /// Writes `data` to the device like any program does
    fn write(&self, position: usize, data: &[u8]) {
        write_to(Path::new(&self.path), position, data);
    }

    /// Changes the data behind the back of the device, as if the device had lost what was
    /// written to it. The system doesn't notice that, so what it has cached of the device
    /// stays as it is.
    fn lose_data(&self, position: usize, num_bytes: usize) {
        write_to(&self.backing_file, position, &vec![0; num_bytes]);
    }
}

impl Drop for LoopDevice {
    fn drop(&mut self) {
        let _ = Command::new("losetup").args(["-d", &self.path]).output();
        // The system only detaches the device when nothing has it open anymore, which can take
        // a moment. Until then, the file system of its file can't be unmounted.
        let name = self.path.trim_start_matches("/dev/");
        let attached = PathBuf::from(format!("/sys/block/{name}/loop/backing_file"));
        wait_until(|| !attached.exists());
    }
}

/// Waits until `condition` is true, but not forever. Returns whether it got true.
fn wait_until(condition: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn write_to(path: &Path, position: usize, data: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(position as u64)).unwrap();
    file.write_all(data).unwrap();
    file.sync_all().unwrap();
}

/// Data that isn't the data of any seed, to see what a wipe left alone
fn other_data(num_bytes: usize) -> Vec<u8> {
    (0..num_bytes).map(|i| (i % 251) as u8 + 1).collect()
}

/// Runs hddwiper with `args`, and with blocks of `block_size`
fn hddwiper_with_blocks(block_size: &str, args: &[&str]) -> Run {
    let mut all_args = vec!["--blocksize", block_size];
    all_args.extend(args);
    hddwiper_with_exactly(&all_args)
}

/// Wipes `path` deterministically with blocks of `block_size` and returns the seed. The wipe
/// has to succeed.
fn wipe_deterministically(path: &str, block_size: &str, options: &[&str]) -> String {
    let mut args = vec!["--deterministic"];
    args.extend(options);
    args.push(path);

    let run = hddwiper_with_blocks(block_size, &args);

    assert_eq!(0, run.exit_code, "{}", run.stderr);
    assert!(run.stdout.ends_with("Finished.\n"), "{}", run.stdout);
    shown_seed(&run).to_string()
}

#[test]
#[ignore = "needs root"]
fn wipe_writes_the_data_of_its_seed_to_the_whole_device() {
    let dir = TestDir::new("wipe-device");
    // Devices that end at the end of a write of the wipe, and within one. And the two sizes
    // that sectors commonly have. The larger ones take several reads of the verification.
    for (sector_size, size) in [
        (512, 8 * MIB),
        (512, 8 * MIB + 512),
        (512, 3 * MIB + 1536),
        (512, 24 * MIB + 512),
        (512, 32 * MIB),
        (512, 40 * MIB + 512),
        (4096, 8 * MIB),
        (4096, 8 * MIB + 8192),
        (4096, 24 * MIB + 4096),
    ] {
        for block_size in BLOCK_SIZES {
            let device = LoopDevice::new(dir.path("device"), size, sector_size);

            let seed = wipe_deterministically(&device.path, block_size, &[]);

            assert!(
                data_of_seed(&seed, 0, size) == device.content(),
                "Wrong data on a device of {size} bytes with sectors of {sector_size} bytes, \
                 wiped with blocks of {block_size}"
            );
            let run = hddwiper(&["--verify", &seed, &device.path]);
            assert_eq!(0, run.exit_code, "{}", run.stderr);
            assert!(
                run.stdout
                    .contains(&format!("Verification successful: All {size} bytes")),
                "{}",
                run.stdout
            );
        }
    }
}

#[test]
#[ignore = "needs root"]
fn wipe_without_seed_fills_the_whole_device() {
    const SIZE: usize = 8 * MIB + 512;
    let dir = TestDir::new("wipe-random");
    for options in [vec![], vec!["--disable-rdrand"]] {
        for block_size in BLOCK_SIZES {
            let device = LoopDevice::new(dir.path("device"), SIZE, 512);

            let mut args = options.clone();
            args.push(&device.path);
            let run = hddwiper_with_blocks(block_size, &args);

            assert_eq!(0, run.exit_code, "{options:?}: {}", run.stderr);
            assert!(run.stdout.ends_with("Finished.\n"), "{}", run.stdout);
            // We can't know the data. But random data has no sector of zeroes, so no sector was
            // left out, up to the last one.
            let content = device.content();
            assert_eq!(SIZE, content.len());
            for (index, sector) in content.chunks(512).enumerate() {
                assert!(
                    sector.iter().any(|byte| *byte != 0),
                    "{options:?}, blocks of {block_size}: Sector {index} wasn't written"
                );
            }
        }
    }
}

#[test]
#[ignore = "needs root"]
fn wipe_leaves_alone_what_is_in_front_of_skip_bytes() {
    const SIZE: usize = 8 * MIB;
    let dir = TestDir::new("wipe-skip");
    // At the beginning of a sector, and within one. Then the wipe has to write the beginning
    // of that sector again as it is.
    for skip_bytes in [4096, 5000, 4 * MIB, 4 * MIB + 1] {
        for block_size in BLOCK_SIZES {
            let device = LoopDevice::new(dir.path("device"), SIZE, 512);
            device.write(0, &other_data(SIZE));
            let skip_bytes_arg = skip_bytes.to_string();

            let seed = wipe_deterministically(
                &device.path,
                block_size,
                &["--skip-bytes", &skip_bytes_arg],
            );

            let content = device.content();
            assert!(
                other_data(SIZE)[..skip_bytes] == content[..skip_bytes],
                "Data in front of {skip_bytes} skipped bytes was changed, with blocks of \
                 {block_size}"
            );
            assert!(
                data_of_seed(&seed, skip_bytes as u64, SIZE - skip_bytes) == content[skip_bytes..],
                "Wrong data behind {skip_bytes} skipped bytes, with blocks of {block_size}"
            );
            let run = hddwiper(&[
                "--verify",
                &seed,
                "--skip-bytes",
                &skip_bytes_arg,
                &device.path,
            ]);
            assert_eq!(0, run.exit_code, "{}", run.stderr);
        }
    }
}

#[test]
#[ignore = "needs root"]
fn interrupted_wipe_can_be_continued_with_its_seed() {
    const SIZE: usize = 8 * MIB;
    let dir = TestDir::new("wipe-continue");
    for block_size in BLOCK_SIZES {
        let device = LoopDevice::new(dir.path("device"), SIZE, 512);
        let seed = wipe_deterministically(&device.path, block_size, &[]);
        // As if the wipe had been interrupted here
        device.write(3 * MIB + 1234, &vec![0; SIZE - 3 * MIB - 1234]);

        // It is continued a bit before that, at a position within a sector
        let run = hddwiper_with_blocks(
            block_size,
            &[
                &format!("--deterministic={seed}"),
                "--skip-bytes",
                "3000001",
                &device.path,
            ],
        );

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(
            data_of_seed(&seed, 0, SIZE) == device.content(),
            "Wrong data with blocks of {block_size}"
        );
        let run = hddwiper(&["--verify", &seed, &device.path]);
        assert_eq!(0, run.exit_code, "{}", run.stdout);
    }
}

#[test]
#[ignore = "needs root"]
fn verify_finds_data_that_was_overwritten() {
    const SIZE: usize = 8 * MIB;
    let dir = TestDir::new("verify-overwritten");
    let device = LoopDevice::new(dir.path("device"), SIZE, 512);
    let seed = wipe_deterministically(&device.path, "64K", &[]);
    device.write(MIB, &vec![0; 10_000]);

    let run = hddwiper(&["--verify", &seed, &device.path]);

    // The three sectors that the 10000 bytes are in
    assert_eq!(1, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains(&format!("Verification FAILED: 12288 of {SIZE} bytes")),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  1048576 - 1060863\n"),
        "{}",
        run.stdout
    );
}

#[test]
#[ignore = "needs root"]
fn verify_finds_data_that_was_overwritten_where_two_reads_meet() {
    // A verification reads 16 MiB at a time, so this takes three reads
    const SIZE: usize = 40 * MIB;
    let dir = TestDir::new("verify-overwritten-between-reads");
    let device = LoopDevice::new(dir.path("device"), SIZE, 512);
    let seed = wipe_deterministically(&device.path, "10M", &[]);
    // The end of the first read and the beginning of the second one
    device.write(16 * MIB - 100, &[0; 200]);
    // And the last byte of the last read
    let last_byte = device.content()[SIZE - 1];
    device.write(SIZE - 1, &[!last_byte]);

    let run = hddwiper(&["--verify", &seed, &device.path]);

    // The two sectors that the 200 bytes are in are one part, although two reads found it
    assert_eq!(1, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains(&format!("Verification FAILED: 12288 of {SIZE} bytes")),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("  16773120 - 16781311\n  41938944 - 41943039\n"),
        "{}",
        run.stdout
    );
}

#[test]
#[ignore = "needs root"]
fn verify_finds_data_that_the_device_lost_although_the_system_has_it_cached() {
    const SIZE: usize = 8 * MIB;
    let dir = TestDir::new("verify-cached");
    let device = LoopDevice::new(dir.path("device"), SIZE, 512);
    // While anything has the device open, the system keeps what it has cached of it
    let _keeps_device_open = File::open(&device.path).unwrap();
    let seed = wipe_deterministically(&device.path, "64K", &[]);
    // Reading the device puts its data into the cache of the system
    assert!(data_of_seed(&seed, 0, SIZE) == device.content());

    device.lose_data(2 * MIB, SIZE - 2 * MIB);

    // A verification that read through the cache would get the data that was written, and
    // would call the device fine
    let run = hddwiper(&["--verify", &seed, &device.path]);
    assert_eq!(1, run.exit_code, "{}", run.stdout);
    assert!(
        run.stdout.contains(&format!(
            "Verification FAILED: {} of {SIZE} bytes",
            SIZE - 2 * MIB
        )),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains(&format!("  {} - {}\n", 2 * MIB, SIZE - 1)),
        "{}",
        run.stdout
    );
}

#[test]
#[ignore = "needs root"]
fn wipe_fails_if_the_device_has_less_space_than_it_claims() {
    const CLAIMED_SIZE: usize = 16 * MIB;
    const REAL_SIZE: usize = 4 * MIB;
    let dir = TestDir::new("wipe-less-space");
    // The device keeps its data in a file system that is too small for it
    let file_system = FileSystem::in_memory(dir.path("mnt"), REAL_SIZE);
    for options in [vec![], vec!["--deterministic"]] {
        // The space ends at the end of a block, and within one
        for block_size in BLOCK_SIZES {
            let device = LoopDevice::new(file_system.path("device"), CLAIMED_SIZE, 512);

            let mut args = options.clone();
            args.push(&device.path);
            let run = hddwiper_with_blocks(block_size, &args);

            assert_eq!(1, run.exit_code, "{options:?}: {}", run.stdout);
            assert!(!run.stdout.contains("Finished."), "{}", run.stdout);
            assert!(
                run.stderr.contains(&format!(
                    "Error: The wipe of {} ended after {REAL_SIZE} bytes, although its size is \
                     {CLAIMED_SIZE} bytes",
                    device.path
                )),
                "{options:?}, blocks of {block_size}: {}",
                run.stderr
            );
        }
    }
}

#[test]
#[ignore = "needs root"]
fn wipe_fills_a_file_until_its_file_system_is_full() {
    // Not a whole number of the larger blocks
    const SIZE: usize = 9 * MIB;
    let dir = TestDir::new("wipe-file");
    let file_system = FileSystem::in_memory(dir.path("mnt"), SIZE);
    let file = file_system.path("file");
    let file = file.to_str().unwrap();

    for block_size in BLOCK_SIZES {
        let seed = wipe_deterministically(file, block_size, &[]);

        // The file is all that is in the file system, so it got all the space
        assert!(
            data_of_seed(&seed, 0, SIZE) == std::fs::read(file).unwrap(),
            "Wrong data with blocks of {block_size}"
        );
        let run = hddwiper(&["--verify", &seed, file]);
        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(
            run.stdout
                .contains(&format!("Verification successful: All {SIZE} bytes")),
            "{}",
            run.stdout
        );
    }
}

#[test]
#[ignore = "needs root"]
fn wipe_fills_a_file_on_a_file_system_of_a_device() {
    const DEVICE_SIZE: usize = 32 * MIB;
    let dir = TestDir::new("wipe-file-on-device");
    let device = LoopDevice::new(dir.path("device"), DEVICE_SIZE, 512);
    let file_system = FileSystem::on_device(dir.path("mnt"), &device);
    let file = file_system.path("file");
    let file = file.to_str().unwrap();

    for block_size in BLOCK_SIZES {
        let seed = wipe_deterministically(file, block_size, &[]);

        // How much of the device the file gets is up to the file system, but it is most of it
        let content = std::fs::read(file).unwrap();
        let size = content.len();
        assert!(size > DEVICE_SIZE / 2, "The file only has {size} bytes");
        assert!(
            data_of_seed(&seed, 0, size) == content,
            "Wrong data with blocks of {block_size}"
        );
        let run = hddwiper(&["--verify", &seed, file]);
        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(
            run.stdout
                .contains(&format!("Verification successful: All {size} bytes")),
            "{}",
            run.stdout
        );
    }
}
