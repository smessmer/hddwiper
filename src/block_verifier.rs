use anyhow::{ensure, Context, Result};
use std::io::{self, ErrorKind, Read};
use std::ops::Range;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

use crate::producer::ProductReceiver;

/// Differences are located in units of this many bytes. A sector counts as differing as a whole
/// if any of its bytes differs. Comparing byte by byte instead would split a part of the device
/// that has the wrong data into many small pieces, since some of its bytes match by chance.
pub const SECTOR_SIZE: u64 = 4096;

/// A [VerificationResult] lists at most this many of the parts that differ. There can be so many
/// of them that a list of all of them wouldn't fit into memory.
pub const MAX_LISTED_MISMATCHES: usize = 100;

/// What a [BlockVerifier] found
#[derive(Debug, PartialEq, Eq)]
pub struct VerificationResult {
    /// The number of bytes that were compared
    pub num_bytes_verified: u64,

    /// The number of bytes in sectors that differ from the expected data
    pub num_bytes_mismatched: u64,

    /// The number of parts that differ. Sectors that follow each other directly are one part.
    pub num_mismatches: u64,

    /// The first parts that differ, as ranges of positions in bytes.
    /// There are at most [MAX_LISTED_MISMATCHES] of them.
    pub first_mismatches: Vec<Range<u64>>,
}

/// Launches a thread that reads from a [Read], e.g. a [std::fs::File], and compares what it reads
/// with the blocks from a [ProductReceiver].
pub struct BlockVerifier {
    join_handle: JoinHandle<Result<VerificationResult>>,
    num_bytes_verified: Arc<AtomicU64>,
}

impl BlockVerifier {
    /// Launch a new thread that compares the content of `reader` with the blocks from `block_source`
    /// until `reader` is at its end. `start_position` is the position that `reader` is at in the
    /// beginning. It is only used to say where the differences are.
    pub fn new(
        block_source: impl 'static + ProductReceiver<Vec<u8>> + Send,
        reader: impl 'static + Read + Send,
        start_position: u64,
    ) -> Self {
        let num_bytes_verified = Arc::new(AtomicU64::new(0));
        let join_handle = {
            let num_bytes_verified = Arc::clone(&num_bytes_verified);
            thread::spawn(move || {
                _verify_blocks(block_source, reader, start_position, &num_bytes_verified)
            })
        };
        Self {
            join_handle,
            num_bytes_verified,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }

    pub fn num_bytes_verified(&self) -> u64 {
        self.num_bytes_verified.load(Ordering::Relaxed)
    }

    /// Waits until the verification is finished and returns what it found. This fails if the
    /// content couldn't be read or the expected blocks couldn't be produced.
    pub fn join(self) -> Result<VerificationResult> {
        self.join_handle.join().unwrap()
    }
}

fn _verify_blocks(
    block_source: impl ProductReceiver<Vec<u8>>,
    mut reader: impl Read,
    start_position: u64,
    num_bytes_verified: &AtomicU64,
) -> Result<VerificationResult> {
    let mut mismatches = Mismatches::new(start_position);
    let mut actual = Vec::new();
    loop {
        log::debug!("Getting block...");
        let expected = block_source.blocking_get_product()?;
        // With an empty block, we couldn't tell if the reader is at its end
        ensure!(!expected.is_empty(), "Can't verify with empty blocks");
        actual.resize(expected.len(), 0);
        log::debug!("Getting block...reading block...");
        let num_read = _read_until_full_or_end(&mut reader, &mut actual).with_context(|| {
            format!(
                "Failed to read the data at position {}",
                mismatches.position
            )
        })?;
        mismatches.compare(&expected[..num_read], &actual[..num_read]);
        num_bytes_verified.fetch_add(num_read as u64, Ordering::Relaxed);
        log::debug!("Getting block...reading block...finished");
        if num_read < expected.len() {
            // The reader is at its end
            return Ok(mismatches.into_result());
        }
    }
}

/// Reads until `buffer` is full or `reader` is at its end. Returns the number of bytes read.
fn _read_until_full_or_end(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut num_read = 0;
    while num_read < buffer.len() {
        match reader.read(&mut buffer[num_read..]) {
            Ok(0) => break,
            Ok(n) => num_read += n,
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(num_read)
}

/// Collects the parts that differ, from data that is compared piece by piece
struct Mismatches {
    start_position: u64,

    /// The position of the next data to compare
    position: u64,

    /// Where the sector begins that the data at `position` belongs to. Sectors are aligned to
    /// the device, so the first one we get can begin in its middle.
    sector_start: u64,

    /// Whether the part of that sector that was compared already differs
    sector_differs: bool,

    num_bytes: u64,
    num_ranges: u64,
    first_ranges: Vec<Range<u64>>,

    /// The latest part that differs. It isn't counted yet, because it grows as long as the
    /// sectors that follow it directly differ as well.
    current_range: Option<Range<u64>>,
}

impl Mismatches {
    fn new(start_position: u64) -> Self {
        Self {
            start_position,
            position: start_position,
            sector_start: start_position,
            sector_differs: false,
            num_bytes: 0,
            num_ranges: 0,
            first_ranges: Vec::new(),
            current_range: None,
        }
    }

    /// Compares the data that is expected at the current position with the data that is there.
    /// The result doesn't depend on how the data is split into calls of this function.
    fn compare(&mut self, expected: &[u8], actual: &[u8]) {
        assert_eq!(expected.len(), actual.len());
        // The usual case, and much faster to find out than by looking at each sector
        let all_matches = expected == actual;
        let mut offset = 0;
        while offset < expected.len() {
            let sector_end = (self.position / SECTOR_SIZE + 1) * SECTOR_SIZE;
            let len = (expected.len() - offset).min((sector_end - self.position) as usize);
            if !all_matches && expected[offset..offset + len] != actual[offset..offset + len] {
                self.sector_differs = true;
            }
            offset += len;
            self.position += len as u64;
            if self.position == sector_end {
                self.finish_sector();
            }
        }
    }

    /// Called when all data of the current sector was compared
    fn finish_sector(&mut self) {
        if self.sector_differs {
            self.add_to_current_range(self.sector_start..self.position);
        }
        self.sector_start = self.position;
        self.sector_differs = false;
    }

    fn add_to_current_range(&mut self, sector: Range<u64>) {
        self.num_bytes += sector.end - sector.start;
        match &mut self.current_range {
            Some(current_range) if current_range.end == sector.start => {
                current_range.end = sector.end;
            }
            _ => {
                self.finish_current_range();
                self.current_range = Some(sector);
            }
        }
    }

    fn finish_current_range(&mut self) {
        if let Some(range) = self.current_range.take() {
            self.num_ranges += 1;
            if self.first_ranges.len() < MAX_LISTED_MISMATCHES {
                self.first_ranges.push(range);
            }
        }
    }

    fn into_result(mut self) -> VerificationResult {
        // The data can end in the middle of a sector
        self.finish_sector();
        self.finish_current_range();
        VerificationResult {
            num_bytes_verified: self.position - self.start_position,
            num_bytes_mismatched: self.num_bytes,
            num_mismatches: self.num_ranges,
            first_mismatches: self.first_ranges,
        }
    }
}

#[cfg(test)]
mod tests {
    // The lists of mismatches we expect often have exactly one range, which clippy takes for a mistake
    #![allow(clippy::single_range_in_vec_init)]

    use super::*;
    use anyhow::anyhow;
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::sync::Mutex;

    const BLOCK_SIZE: usize = 10_000;

    /// A block source that hands out the given blocks and then fails
    struct FixedBlocks {
        blocks: Mutex<VecDeque<Vec<u8>>>,
    }

    impl FixedBlocks {
        fn new(data: &[u8], block_size: usize) -> Self {
            Self {
                blocks: Mutex::new(
                    data.chunks(block_size)
                        .map(|block| block.to_vec())
                        .collect(),
                ),
            }
        }
    }

    impl ProductReceiver<Vec<u8>> for FixedBlocks {
        fn blocking_get_product(&self) -> Result<Vec<u8>> {
            let mut blocks = self.blocks.lock().unwrap();
            blocks.pop_front().ok_or_else(|| anyhow!("No blocks left"))
        }

        fn get_all_available_products(&self) -> Vec<Vec<u8>> {
            self.blocks.lock().unwrap().drain(..).collect()
        }

        fn num_products_in_buffer(&self) -> usize {
            self.blocks.lock().unwrap().len()
        }
    }

    /// Some data that doesn't repeat, so that no two sectors of it are the same
    fn test_data(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).to_le_bytes()[7])
            .collect()
    }

    /// Verifies `actual` against `expected`. There have to be more expected bytes than actual
    /// ones, like in real life, where blocks get produced for as long as somebody takes them.
    fn verify_with_block_size(
        expected: &[u8],
        actual: Vec<u8>,
        start_position: u64,
        block_size: usize,
    ) -> VerificationResult {
        assert!(expected.len() >= actual.len() + block_size);
        let block_source = FixedBlocks::new(expected, block_size);
        BlockVerifier::new(block_source, Cursor::new(actual), start_position)
            .join()
            .unwrap()
    }

    fn verify_at(expected: &[u8], actual: Vec<u8>, start_position: u64) -> VerificationResult {
        verify_with_block_size(expected, actual, start_position, BLOCK_SIZE)
    }

    fn verify(expected: &[u8], actual: Vec<u8>) -> VerificationResult {
        verify_at(expected, actual, 0)
    }

    fn successful_result(num_bytes_verified: u64) -> VerificationResult {
        VerificationResult {
            num_bytes_verified,
            num_bytes_mismatched: 0,
            num_mismatches: 0,
            first_mismatches: vec![],
        }
    }

    #[test]
    fn same_data_has_no_mismatches() {
        let expected = test_data(100_000);

        // The data ends in the middle of a block, ...
        let actual = expected[..34_567].to_vec();
        assert_eq!(successful_result(34_567), verify(&expected, actual));

        // ...at the end of a block, ...
        let actual = expected[..3 * BLOCK_SIZE].to_vec();
        assert_eq!(
            successful_result(3 * BLOCK_SIZE as u64),
            verify(&expected, actual)
        );

        // ...or right away
        assert_eq!(successful_result(0), verify(&expected, vec![]));
    }

    #[test]
    fn one_wrong_byte_is_a_mismatch_of_its_sector() {
        let expected = test_data(100_000);
        let mut actual = expected[..50_000].to_vec();
        actual[10_000] ^= 1;

        assert_eq!(
            VerificationResult {
                num_bytes_verified: 50_000,
                num_bytes_mismatched: SECTOR_SIZE,
                num_mismatches: 1,
                first_mismatches: vec![8192..12_288],
            },
            verify(&expected, actual)
        );
    }

    #[test]
    fn sectors_that_follow_each_other_are_one_mismatch() {
        let expected = test_data(100_000);
        let mut actual = expected[..50_000].to_vec();
        // Three sectors in a row, which also are in two different blocks, and another sector
        for position in [4096, 8192 + 100, 12_288 + 4095, 30_000] {
            actual[position] ^= 1;
        }

        assert_eq!(
            VerificationResult {
                num_bytes_verified: 50_000,
                num_bytes_mismatched: 4 * SECTOR_SIZE,
                num_mismatches: 2,
                first_mismatches: vec![4096..16_384, 28_672..32_768],
            },
            verify(&expected, actual)
        );
    }

    #[test]
    fn mismatches_dont_reach_beyond_the_verified_data() {
        let expected = test_data(100_000);

        // The verified data begins and ends in the middle of a sector
        let mut actual = expected[..9000].to_vec();
        actual[0] ^= 1;
        actual[8999] ^= 1;

        assert_eq!(
            VerificationResult {
                num_bytes_verified: 9000,
                num_bytes_mismatched: (4096 - 1000) + (10_000 - 8192),
                num_mismatches: 2,
                first_mismatches: vec![1000..4096, 8192..10_000],
            },
            verify_at(&expected, actual, 1000)
        );
    }

    #[test]
    fn positions_of_mismatches_are_positions_on_the_device() {
        let expected = test_data(100_000);
        let mut actual = expected[..50_000].to_vec();
        actual[5000] ^= 1;

        // Byte 5000 of the verified data is byte 1_005_000 of the device
        let start_position = 1_000_000;
        assert_eq!(
            VerificationResult {
                num_bytes_verified: 50_000,
                num_bytes_mismatched: SECTOR_SIZE,
                num_mismatches: 1,
                first_mismatches: vec![1_003_520..1_007_616],
            },
            verify_at(&expected, actual, start_position)
        );
    }

    #[test]
    fn result_doesnt_depend_on_block_size() {
        let expected = test_data(100_000);
        let mut actual = expected[..50_000].to_vec();
        for position in [0, 4095, 9999, 10_000, 20_480, 33_333, 49_999] {
            actual[position] ^= 1;
        }

        // The block sizes don't have to be multiples of the sector size
        let result = verify_with_block_size(&expected, actual.clone(), 1000, 1);
        assert_eq!(4, result.num_mismatches);
        for block_size in [100, 4096, 5000, 10_000, 50_000] {
            assert_eq!(
                result,
                verify_with_block_size(&expected, actual.clone(), 1000, block_size),
                "Different result with block size {block_size}"
            );
        }
    }

    #[test]
    fn completely_different_data_is_one_mismatch() {
        let expected = test_data(100_000);
        let actual = vec![0; 50_000];

        assert_eq!(
            VerificationResult {
                num_bytes_verified: 50_000,
                num_bytes_mismatched: 50_000,
                num_mismatches: 1,
                first_mismatches: vec![0..50_000],
            },
            verify(&expected, actual)
        );
    }

    #[test]
    fn only_the_first_mismatches_are_listed() {
        let num_sectors = 2 * MAX_LISTED_MISMATCHES + 50;
        let expected = test_data((num_sectors + 3) * SECTOR_SIZE as usize);
        let mut actual = expected[..num_sectors * SECTOR_SIZE as usize].to_vec();
        // Every second sector is wrong
        let wrong_sectors = (0..num_sectors as u64).step_by(2);
        for sector in wrong_sectors.clone() {
            actual[(sector * SECTOR_SIZE) as usize] ^= 1;
        }

        let result = verify(&expected, actual);

        assert_eq!(wrong_sectors.clone().count() as u64, result.num_mismatches);
        assert_eq!(
            wrong_sectors.clone().count() as u64 * SECTOR_SIZE,
            result.num_bytes_mismatched
        );
        let first_wrong_sectors: Vec<Range<u64>> = wrong_sectors
            .take(MAX_LISTED_MISMATCHES)
            .map(|sector| sector * SECTOR_SIZE..(sector + 1) * SECTOR_SIZE)
            .collect();
        assert_eq!(first_wrong_sectors, result.first_mismatches);
    }

    /// A reader that only hands out a few bytes at a time and gets interrupted in between
    struct ReluctantReader {
        data: Cursor<Vec<u8>>,
        num_reads: usize,
    }

    impl Read for ReluctantReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.num_reads += 1;
            if self.num_reads % 3 == 0 {
                return Err(io::Error::new(ErrorKind::Interrupted, "Interrupted"));
            }
            let len = buffer.len().min(1 + self.num_reads % 700);
            self.data.read(&mut buffer[..len])
        }
    }

    #[test]
    fn reads_that_return_less_than_a_block_are_continued() {
        let expected = test_data(100_000);
        let mut actual = expected[..34_567].to_vec();
        actual[10_000] ^= 1;
        let reader = ReluctantReader {
            data: Cursor::new(actual),
            num_reads: 0,
        };

        let result = BlockVerifier::new(FixedBlocks::new(&expected, BLOCK_SIZE), reader, 0)
            .join()
            .unwrap();

        assert_eq!(
            VerificationResult {
                num_bytes_verified: 34_567,
                num_bytes_mismatched: SECTOR_SIZE,
                num_mismatches: 1,
                first_mismatches: vec![8192..12_288],
            },
            result
        );
    }

    /// A reader that fails once it handed out its data
    struct BrokenReader {
        data: Cursor<Vec<u8>>,
    }

    impl Read for BrokenReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.data.read(buffer)? {
                0 => Err(io::Error::other("Simulated failure")),
                num_read => Ok(num_read),
            }
        }
    }

    #[test]
    fn fails_if_reading_fails() {
        let expected = test_data(100_000);
        let reader = BrokenReader {
            data: Cursor::new(expected[..25_000].to_vec()),
        };

        let error = BlockVerifier::new(FixedBlocks::new(&expected, BLOCK_SIZE), reader, 1000)
            .join()
            .unwrap_err();

        // It says which block couldn't be read
        assert_eq!(
            "Failed to read the data at position 21000",
            error.to_string()
        );
    }

    #[test]
    fn fails_if_there_are_no_blocks_to_compare_with() {
        let expected = test_data(2 * BLOCK_SIZE);
        let actual = test_data(5 * BLOCK_SIZE);

        let result = BlockVerifier::new(
            FixedBlocks::new(&expected, BLOCK_SIZE),
            Cursor::new(actual),
            0,
        )
        .join();

        assert!(result.is_err());
    }

    #[test]
    fn fails_with_empty_blocks() {
        let block_source = FixedBlocks {
            blocks: Mutex::new(VecDeque::from([vec![]])),
        };

        let result = BlockVerifier::new(block_source, Cursor::new(vec![1, 2, 3]), 0).join();

        assert!(result.is_err());
    }

    #[test]
    fn num_bytes_verified_is_updated() {
        let expected = test_data(100_000);
        let actual = expected[..34_567].to_vec();

        let verifier = BlockVerifier::new(
            FixedBlocks::new(&expected, BLOCK_SIZE),
            Cursor::new(actual),
            0,
        );
        while !verifier.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(34_567, verifier.num_bytes_verified());
    }

    /// What a verification has to find, computed in the simplest way: with all the data at
    /// hand, sector by sector
    fn expected_result(expected: &[u8], actual: &[u8], start_position: u64) -> VerificationResult {
        let end_position = start_position + actual.len() as u64;
        let mut mismatches: Vec<Range<u64>> = vec![];
        let mut sector_start = start_position;
        while sector_start < end_position {
            let sector_end = ((sector_start / SECTOR_SIZE + 1) * SECTOR_SIZE).min(end_position);
            let sector_in_data =
                (sector_start - start_position) as usize..(sector_end - start_position) as usize;
            if expected[sector_in_data.clone()] != actual[sector_in_data] {
                match mismatches.last_mut() {
                    Some(mismatch) if mismatch.end == sector_start => mismatch.end = sector_end,
                    _ => mismatches.push(sector_start..sector_end),
                }
            }
            sector_start = sector_end;
        }
        VerificationResult {
            num_bytes_verified: actual.len() as u64,
            num_bytes_mismatched: mismatches.iter().map(|range| range.end - range.start).sum(),
            num_mismatches: mismatches.len() as u64,
            first_mismatches: mismatches.into_iter().take(MAX_LISTED_MISMATCHES).collect(),
        }
    }

    #[test]
    fn finds_the_mismatches_of_random_data() {
        use rand::{rngs::StdRng, Rng, SeedableRng};

        let mut num_cases_with_unlisted_mismatches = 0;
        // Each case has its own seed, so that one that fails can be run alone
        for case in 0..150 {
            let mut rng = StdRng::seed_from_u64(case);

            // Most cases are small. Every fifth one is large and has many changes, so that it
            // has more mismatches than are listed.
            let is_large = case % 5 == 0;
            let len = if is_large {
                rng.gen_range(1_500_000..=3_000_000)
            } else {
                rng.gen_range(0..=50_000)
            };
            let block_size = rng.gen_range(1..=100_000);
            let start_position = match rng.gen_range(0..3) {
                0 => 0,
                1 => rng.gen_range(0..3 * SECTOR_SIZE),
                _ => rng.gen_range(0..1 << 45),
            };

            let expected = test_data(len + 2 * block_size);
            let mut actual = expected[..len].to_vec();
            // Changes of all sizes, some of them next to each other or overlapping
            let num_changes = match len {
                0 => 0,
                _ if is_large => rng.gen_range(200..600),
                _ => rng.gen_range(0..50),
            };
            for _ in 0..num_changes {
                let change_start = rng.gen_range(0..len);
                let change_len = match rng.gen_range(0..10) {
                    0 => rng.gen_range(1..=3 * SECTOR_SIZE as usize),
                    _ => 1,
                };
                for byte in &mut actual[change_start..len.min(change_start + change_len)] {
                    *byte ^= 0xff;
                }
            }

            let expected_result = expected_result(&expected, &actual, start_position);
            if expected_result.num_mismatches > MAX_LISTED_MISMATCHES as u64 {
                num_cases_with_unlisted_mismatches += 1;
            }
            assert_eq!(
                expected_result,
                verify_with_block_size(&expected, actual, start_position, block_size),
                "Case {case}: {len} bytes at position {start_position}, compared in blocks of \
                 {block_size} bytes"
            );
        }
        // Otherwise this wouldn't test what it is meant to
        assert!(
            num_cases_with_unlisted_mismatches >= 10,
            "Only {num_cases_with_unlisted_mismatches} cases had mismatches that aren't listed"
        );
    }
}
