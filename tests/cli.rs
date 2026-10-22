//! Tests that run the hddwiper binary like a user does

mod common;

use common::{data_of_seed, hddwiper, hddwiper_with_exactly};
use std::path::PathBuf;

/// A seed, and the first 64 bytes of the data that belongs to it. Like in the unit test for
/// this, these bytes were computed with libsodium. They are XSalsa20 with the seed as its key
/// and a nonce of zeroes.
const SEED: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const FIRST_DATA_OF_SEED: &str = "a141cb3246c2dd6bfbb8aedfaee2caf0d6c7d5fd72abfca1ee1cbbbd94058ddc\
                                  4a0ef04198aabf335c54720f2546bed2e43474f047e6284c6b034b8d3cc9df32";
const OTHER_SEED: &str = "ff0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

/// A file for a test. It is removed when this is dropped.
struct TempFile(PathBuf);

impl TempFile {
    /// Creates the file with `content`. Each test has to use another `name`.
    fn new(name: &str, content: &[u8]) -> Self {
        let file = Self::missing(name);
        std::fs::write(&file.0, content).unwrap();
        file
    }

    /// The path of a file that doesn't exist
    fn missing(name: &str) -> Self {
        let file_name = format!("hddwiper-cli-test-{name}-{}", std::process::id());
        Self(std::env::temp_dir().join(file_name))
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }

    fn content(&self) -> Vec<u8> {
        std::fs::read(&self.0).unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn verify_succeeds_for_the_data_of_the_seed() {
    // Not a whole number of sectors, like a file can be
    let data = data_of_seed(SEED, 0, 20_000);
    let file = TempFile::new("verify-succeeds", &data);

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(0, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification successful: All 20000 bytes"),
        "{}",
        run.stdout
    );
    // A verification doesn't write anything
    assert!(data == file.content());
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "blocks of the default size take seconds unoptimized"
)]
fn verify_succeeds_with_default_options() {
    let data = data_of_seed(SEED, 0, 20_000);
    let file = TempFile::new("verify-defaults", &data);

    let run = hddwiper_with_exactly(&["--verify", SEED, file.path()]);

    assert_eq!(0, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification successful: All 20000 bytes"),
        "{}",
        run.stdout
    );
}

#[test]
fn verify_succeeds_for_the_data_that_libsodium_computed() {
    // The other tests compute the data of a seed with the library that hddwiper uses too. This
    // one doesn't, so it also notices if a seed ever stands for other data.
    let data = hex::decode(FIRST_DATA_OF_SEED).unwrap();
    assert!(data == data_of_seed(SEED, 0, 64));
    let file = TempFile::new("verify-libsodium", &data);

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(0, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout.contains("Verification successful: All 64 bytes"),
        "{}",
        run.stdout
    );
}

#[test]
fn verify_fails_for_changed_data() {
    let mut data = data_of_seed(SEED, 0, 20_000);
    data[10_000] ^= 1;
    let file = TempFile::new("verify-changed", &data);

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(1, run.exit_code, "{}", run.stderr);
    // It names the sector with the changed byte
    assert!(
        run.stdout
            .contains("Verification FAILED: 4096 of 20000 bytes"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("  8192 - 12287\n"), "{}", run.stdout);
    assert!(data == file.content());
}

#[test]
fn verify_fails_for_the_data_of_another_seed() {
    let data = data_of_seed(OTHER_SEED, 0, 20_000);
    let file = TempFile::new("verify-other-seed", &data);

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(1, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification FAILED: 20000 of 20000 bytes"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("  0 - 19999\n"), "{}", run.stdout);
}

#[test]
fn verify_fails_for_data_at_the_wrong_position() {
    // The data of the seed, but one byte too late. That is what a device returns that mixes up
    // where it stored what.
    let mut data = vec![0];
    data.extend(data_of_seed(SEED, 0, 19_999));
    let file = TempFile::new("verify-shifted", &data);

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(1, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification FAILED: 20000 of 20000 bytes"),
        "{}",
        run.stdout
    );
}

#[test]
fn verify_only_looks_at_the_data_behind_skip_bytes() {
    // The first 5000 bytes aren't the data of the seed, as if a wipe had skipped them
    let mut data = vec![0; 5000];
    data.extend(data_of_seed(SEED, 5000, 15_000));
    let file = TempFile::new("verify-skip", &data);

    let run = hddwiper(&["--verify", SEED, "--skip-bytes", "5000", file.path()]);
    assert_eq!(0, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification successful: All 15000 bytes"),
        "{}",
        run.stdout
    );

    // Without skipping them, they are found. They end within the second sector.
    let run = hddwiper(&["--verify", SEED, file.path()]);
    assert_eq!(1, run.exit_code, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Verification FAILED: 8192 of 20000 bytes"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("  0 - 8191\n"), "{}", run.stdout);
}

#[test]
fn verify_doesnt_depend_on_the_other_options() {
    let mut data = data_of_seed(SEED, 0, 20_000);
    data[10_000] ^= 1;
    let file = TempFile::new("verify-options", &data);

    for options in [
        vec!["--blocksize", "1"],
        vec!["--blocksize", "1000"],
        vec!["-b", "4K", "--buffersize", "1"],
        vec!["--blocksize=1M", "-u", "3"],
        // This one only matters for a wipe
        vec!["--blocksize", "64K", "--disable-rdrand"],
    ] {
        let mut args = options.clone();
        args.extend(["--verify", SEED, file.path()]);

        let run = hddwiper_with_exactly(&args);

        assert_eq!(1, run.exit_code, "{options:?}: {}", run.stderr);
        assert!(
            run.stdout
                .contains("Verification FAILED: 4096 of 20000 bytes"),
            "{options:?}: {}",
            run.stdout
        );
        assert!(
            run.stdout.contains("  8192 - 12287\n"),
            "{options:?}: {}",
            run.stdout
        );
    }
}

#[test]
fn verify_fails_if_there_is_nothing_to_verify() {
    let data = data_of_seed(SEED, 0, 20_000);
    let file = TempFile::new("verify-nothing", &data);
    let empty_file = TempFile::new("verify-empty", &[]);

    for args in [
        vec!["--verify", SEED, "--skip-bytes", "20000", file.path()],
        vec!["--verify", SEED, "--skip-bytes", "1G", file.path()],
        vec!["--verify", SEED, empty_file.path()],
    ] {
        let run = hddwiper(&args);

        // Saying that everything is fine would be wrong, because nothing was looked at
        assert_eq!(1, run.exit_code, "{args:?}");
        assert!(
            run.stderr.contains("There is nothing to verify"),
            "{args:?}: {}",
            run.stderr
        );
        assert!(!run.stdout.contains("Verification"), "{}", run.stdout);
    }
}

#[test]
fn verify_fails_for_a_file_that_doesnt_exist() {
    let file = TempFile::missing("verify-missing");

    let run = hddwiper(&["--verify", SEED, file.path()]);

    assert_eq!(1, run.exit_code);
    assert!(run.stderr.starts_with("Error: "), "{}", run.stderr);
    // And it didn't create the file
    assert!(!file.0.exists());
}

#[test]
fn invalid_arguments_are_rejected() {
    let file = TempFile::new("invalid-arguments", &data_of_seed(SEED, 0, 20_000));
    let deterministic_with_seed = format!("--deterministic={SEED}");

    for args in [
        // No file
        vec![],
        vec!["--deterministic"],
        vec!["--verify", SEED],
        // A wipe and a verification at once
        vec!["--deterministic", "--verify", SEED, file.path()],
        vec![&deterministic_with_seed, "--verify", SEED, file.path()],
        // Seeds that aren't 64 hex characters
        vec!["--verify", "1234", file.path()],
        vec!["--verify", &SEED[1..], file.path()],
        vec!["--verify", &SEED.replace('0', "g"), file.path()],
        vec!["--deterministic=1234", file.path()],
        vec!["--deterministic=", file.path()],
        // The seed of a wipe has to be attached with a "=". Otherwise it could be the file.
        vec!["--deterministic", SEED, file.path()],
        // No seed
        vec!["--verify", file.path()],
        // Options that don't exist, and one that is there twice
        vec!["--seed", SEED, file.path()],
        vec!["--blocksize", "1M", "--blocksize", "1M", file.path()],
    ] {
        let run = hddwiper_with_exactly(&args);

        assert_eq!(2, run.exit_code, "{args:?}: {}", run.stderr);
        assert!(
            run.stderr.starts_with("error: "),
            "{args:?}: {}",
            run.stderr
        );
        assert!(run.stdout.is_empty(), "{args:?}: {}", run.stdout);
    }
    // None of them touched the file
    assert!(data_of_seed(SEED, 0, 20_000) == file.content());
}

#[test]
fn sizes_that_make_no_sense_are_rejected() {
    let data = data_of_seed(SEED, 0, 20_000);
    let file = TempFile::new("invalid-sizes", &data);

    for (options, message) in [
        (
            vec!["--blocksize", "0"],
            "--blocksize has to be larger than zero",
        ),
        // Less than a byte
        (
            vec!["--blocksize", "0.0001K"],
            "--blocksize has to be larger than zero",
        ),
    ] {
        // For a verification and for a wipe, which must not even begin with them
        for mode in [vec!["--verify", SEED], vec![], vec!["--deterministic"]] {
            let mut args = options.clone();
            args.extend(mode);
            args.push(file.path());

            let run = hddwiper_with_exactly(&args);

            assert_eq!(1, run.exit_code, "{args:?}");
            assert!(run.stderr.contains(message), "{args:?}: {}", run.stderr);
        }
    }
    assert!(data == file.content());
}

#[test]
fn help_explains_the_options() {
    let run = hddwiper_with_exactly(&["--help"]);

    assert_eq!(0, run.exit_code);
    for option in [
        "--skip-bytes",
        "--blocksize",
        "--buffersize",
        "--disable-rdrand",
        "--deterministic[=<SEED>]",
        "--verify <SEED>",
    ] {
        assert!(run.stdout.contains(option), "{option}: {}", run.stdout);
    }
}

#[test]
fn version_is_shown() {
    let run = hddwiper_with_exactly(&["--version"]);

    assert_eq!(0, run.exit_code);
    assert_eq!(
        format!("hddwiper {}\n", env!("CARGO_PKG_VERSION")),
        run.stdout
    );
}

#[test]
fn wipe_fails_if_the_file_cant_be_opened() {
    // A file in a directory that doesn't exist
    let directory = TempFile::missing("wipe-missing-directory");
    let file = directory.0.join("file");

    let run = hddwiper(&[file.to_str().unwrap()]);

    assert_eq!(1, run.exit_code);
    assert!(run.stderr.contains("Error: "), "{}", run.stderr);
    assert!(!run.stdout.contains("Finished."), "{}", run.stdout);
    assert!(!directory.0.exists());
}

/// These tests wipe /dev/full. It never has space for anything, so a wipe of it does everything
/// that a wipe does and ends at once, without root and without a device that we could wipe.
#[cfg(target_os = "linux")]
mod wipe_of_full_device {
    use super::*;
    use common::shown_seed;

    #[test]
    fn wipe_finishes() {
        for options in [
            vec![],
            vec!["--disable-rdrand"],
            vec!["--deterministic"],
            vec!["--buffersize", "2"],
            // A start within a sector
            vec!["--deterministic", "--skip-bytes", "5000"],
        ] {
            let mut args = options.clone();
            args.push("/dev/full");

            let run = hddwiper(&args);

            assert_eq!(0, run.exit_code, "{options:?}: {}", run.stderr);
            assert!(
                run.stdout.ends_with("Finished.\n"),
                "{options:?}: {}",
                run.stdout
            );
        }
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "blocks of the default size take seconds unoptimized"
    )]
    fn wipe_finishes_with_default_options() {
        let run = hddwiper_with_exactly(&["/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(run.stdout.ends_with("Finished.\n"), "{}", run.stdout);
    }

    #[test]
    fn wipe_finishes_with_tiny_blocks() {
        let run = hddwiper_with_exactly(&["--blocksize", "1000", "/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(run.stdout.ends_with("Finished.\n"), "{}", run.stdout);
    }

    #[test]
    fn only_a_deterministic_wipe_shows_a_seed() {
        let run = hddwiper(&["/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(!run.stdout.contains("Seed: "), "{}", run.stdout);
        assert!(!run.stdout.contains("--verify"), "{}", run.stdout);
    }

    #[test]
    fn deterministic_wipe_shows_how_to_verify_it() {
        let run = hddwiper_with_exactly(&["--blocksize", "64K", "--deterministic", "/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        // The command has the options of the wipe, so that it looks at the same data
        let seed = shown_seed(&run);
        let verify_args = format!(" --blocksize 64K --verify {seed} /dev/full\n");
        assert!(run.stdout.contains(&verify_args), "{}", run.stdout);
    }

    #[test]
    fn each_deterministic_wipe_has_another_seed() {
        let run1 = hddwiper(&["--deterministic", "/dev/full"]);
        let run2 = hddwiper(&["--deterministic", "/dev/full"]);

        assert_ne!(shown_seed(&run1), shown_seed(&run2));
    }

    #[test]
    fn deterministic_wipe_takes_a_given_seed() {
        let deterministic_with_seed = format!("--deterministic={SEED}");

        let run = hddwiper(&[&deterministic_with_seed, "/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert_eq!(SEED, shown_seed(&run));
        // Nothing was skipped, so the whole device belongs to this wipe
        assert!(!run.stdout.contains("isn't part of this run"));
    }

    #[test]
    fn continued_deterministic_wipe_says_what_it_doesnt_cover() {
        let deterministic_with_seed = format!("--deterministic={SEED}");

        let run = hddwiper(&[
            &deterministic_with_seed,
            "--skip-bytes",
            "4096",
            "/dev/full",
        ]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert_eq!(SEED, shown_seed(&run));
        assert!(
            run.stdout
                .contains("The data before --skip-bytes isn't part of this run."),
            "{}",
            run.stdout
        );
        let verify_args = format!(" --verify {SEED} --skip-bytes 4096 /dev/full\n");
        assert!(run.stdout.contains(&verify_args), "{}", run.stdout);
    }

    #[test]
    fn seed_of_a_deterministic_wipe_verifies_the_data_of_that_seed() {
        // The wipe couldn't write anything. But what it would have written is the data of the
        // seed it showed, so a file with that data has to pass the verification with that seed.
        let run = hddwiper(&["--deterministic", "/dev/full"]);
        let seed = shown_seed(&run);
        let file = TempFile::new("verify-shown-seed", &data_of_seed(seed, 0, 20_000));

        let run = hddwiper(&["--verify", seed, file.path()]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(
            run.stdout
                .contains("Verification successful: All 20000 bytes"),
            "{}",
            run.stdout
        );
    }

    #[test]
    fn disabling_rdrand_warns() {
        let run = hddwiper(&["--disable-rdrand", "/dev/full"]);

        assert_eq!(0, run.exit_code, "{}", run.stderr);
        assert!(
            run.stderr.contains("RDRAND generator is disabled"),
            "{}",
            run.stderr
        );
    }
}
