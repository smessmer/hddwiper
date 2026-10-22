//! What the tests that run the hddwiper binary have in common

// Each file with tests only uses a part of this
#![allow(dead_code)]

use salsa20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use std::process::Command;

/// What hddwiper did when it was run
pub struct Run {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs hddwiper with `args`, and with small blocks. With the blocks it has by default, a run
/// takes seconds if hddwiper isn't optimized, because of the random data it generates in
/// advance.
pub fn hddwiper(args: &[&str]) -> Run {
    let mut all_args = vec!["--blocksize", "64K"];
    all_args.extend(args);
    hddwiper_with_exactly(&all_args)
}

/// Runs hddwiper with `args` and nothing else
pub fn hddwiper_with_exactly(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_hddwiper"))
        .args(args)
        // So that it logs what it logs by default
        .env_remove("RUST_LOG")
        .output()
        .unwrap();
    Run {
        exit_code: output.status.code().expect("hddwiper was killed"),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The data that a deterministic wipe with `seed` writes to the positions from `position` on
pub fn data_of_seed(seed: &str, position: u64, num_bytes: usize) -> Vec<u8> {
    let key = hex::decode(seed).unwrap();
    let mut cipher = salsa20::XSalsa20::new_from_slices(&key, &[0; 24]).unwrap();
    cipher.seek(position);
    let mut data = vec![0; num_bytes];
    cipher.apply_keystream(&mut data);
    data
}

/// The seed that a deterministic wipe showed
pub fn shown_seed(run: &Run) -> &str {
    let seed = run
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("Seed: "))
        .unwrap_or_else(|| panic!("No seed in: {}", run.stdout));
    assert_eq!(64, seed.len());
    assert!(seed.chars().all(|c| c.is_ascii_hexdigit()));
    seed
}
