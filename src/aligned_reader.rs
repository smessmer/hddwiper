use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::aligned_buffer::{AlignedBuffer, ALIGNMENT};

/// Reads from a reader that only accepts aligned reads, see [ALIGNMENT]. To do that, it reads
/// whole buffers from that reader and hands them out in whatever pieces it is asked for.
pub struct AlignedReader<R> {
    inner: R,
    buffer: AlignedBuffer,

    /// The part of the buffer that was read from `inner` and wasn't handed out yet
    remaining: Range<usize>,

    /// Reads have to begin at an aligned position, which can be in front of the position that
    /// we were asked to read from. This is the number of bytes in between, which get read but
    /// must not be handed out.
    num_bytes_to_skip: usize,

    /// Whether `inner` is at its end
    at_end: bool,
}

impl<R: Read + Seek> AlignedReader<R> {
    /// Creates a reader for the content of `inner` from `position` on. It reads `buffer_size`
    /// bytes at a time, which has to be a multiple of [ALIGNMENT], and begins with that at a
    /// multiple of `buffer_size`. So reads are also aligned for devices with larger sectors.
    pub fn new(mut inner: R, position: u64, buffer_size: usize) -> io::Result<Self> {
        let buffer = AlignedBuffer::new(buffer_size);

        let num_bytes_to_skip = (position % buffer_size as u64) as usize;
        inner.seek(SeekFrom::Start(position - num_bytes_to_skip as u64))?;

        Ok(Self {
            inner,
            buffer,
            remaining: 0..0,
            num_bytes_to_skip,
            at_end: false,
        })
    }
}

impl<R: Read> AlignedReader<R> {
    fn fill_buffer(&mut self) -> io::Result<()> {
        let num_read = self.inner.read(&mut self.buffer)?;
        // Only the end of a file makes a read stop in the middle of a sector. We also couldn't
        // go on behind it, because the next read wouldn't begin at an aligned position.
        self.at_end = num_read == 0 || num_read % ALIGNMENT != 0;
        let num_skipped = self.num_bytes_to_skip.min(num_read);
        self.num_bytes_to_skip -= num_skipped;
        self.remaining = num_skipped..num_read;
        Ok(())
    }
}

impl<R: Read> Read for AlignedReader<R> {
    fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
        // This is a loop because a buffer can consist of nothing but bytes to skip
        while self.remaining.is_empty() && !self.at_end {
            self.fill_buffer()?;
        }
        let num_bytes = dest.len().min(self.remaining.len());
        dest[..num_bytes].copy_from_slice(&self.buffer[self.remaining.start..][..num_bytes]);
        self.remaining.start += num_bytes;
        Ok(num_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    const BUFFER_SIZE: usize = 2 * ALIGNMENT;

    /// Behaves like a file that was opened for direct reads, so it only accepts aligned reads
    struct DirectFile {
        content: Vec<u8>,
        position: usize,

        /// No read returns more bytes than this, no matter how many it was asked for
        max_read_size: usize,

        /// The reads with these numbers fail, counted from zero
        failing_reads: Vec<usize>,
        num_reads: usize,
    }

    impl DirectFile {
        fn new(content: Vec<u8>) -> Self {
            Self {
                content,
                position: 0,
                max_read_size: usize::MAX,
                failing_reads: vec![],
                num_reads: 0,
            }
        }
    }

    impl Read for DirectFile {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            let read_number = self.num_reads;
            self.num_reads += 1;
            if self.failing_reads.contains(&read_number) {
                return Err(io::Error::other("Simulated failure"));
            }

            assert_eq!(
                0,
                self.position % ALIGNMENT,
                "The read doesn't begin at an aligned position"
            );
            assert_eq!(
                0,
                dest.len() % ALIGNMENT,
                "The read doesn't ask for an aligned number of bytes"
            );
            assert_eq!(
                0,
                dest.as_ptr() as usize % ALIGNMENT,
                "The read doesn't read into aligned memory"
            );

            let remaining = &self.content[self.position.min(self.content.len())..];
            let num_bytes = dest.len().min(remaining.len()).min(self.max_read_size);
            dest[..num_bytes].copy_from_slice(&remaining[..num_bytes]);
            self.position += num_bytes;
            Ok(num_bytes)
        }
    }

    impl Seek for DirectFile {
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

    fn content(num_bytes: usize) -> Vec<u8> {
        (0..num_bytes).map(|i| (i % 251) as u8).collect()
    }

    fn read_all(mut reader: impl Read) -> Vec<u8> {
        let mut result = vec![];
        reader.read_to_end(&mut result).unwrap();
        result
    }

    #[test]
    fn reads_the_content() {
        // Contents that end at, right before and right behind the end of a sector and of a buffer
        for num_bytes in [
            0,
            1,
            ALIGNMENT - 1,
            ALIGNMENT,
            ALIGNMENT + 1,
            BUFFER_SIZE - 1,
            BUFFER_SIZE,
            BUFFER_SIZE + 1,
            5 * BUFFER_SIZE,
            5 * BUFFER_SIZE + 1234,
        ] {
            let content = content(num_bytes);
            let reader = AlignedReader::new(DirectFile::new(content.clone()), 0, BUFFER_SIZE);
            assert!(
                content == read_all(reader.unwrap()),
                "Content of {num_bytes} bytes isn't read correctly"
            );
        }
    }

    #[test]
    fn reads_from_the_position() {
        let content = content(5 * BUFFER_SIZE + 1234);
        for position in [
            1,
            ALIGNMENT - 1,
            ALIGNMENT,
            ALIGNMENT + 1,
            BUFFER_SIZE - 1,
            BUFFER_SIZE,
            BUFFER_SIZE + 1,
            3 * BUFFER_SIZE + 567,
            // In the last sector, which the content only fills partly
            5 * BUFFER_SIZE + 1000,
            // The last byte
            content.len() - 1,
        ] {
            let reader = AlignedReader::new(
                DirectFile::new(content.clone()),
                position as u64,
                BUFFER_SIZE,
            );
            assert!(
                content[position..] == read_all(reader.unwrap()),
                "Content from position {position} isn't read correctly"
            );
        }
    }

    #[test]
    fn reads_nothing_from_a_position_at_or_behind_the_end() {
        for num_bytes in [0, 1234, BUFFER_SIZE, 3 * BUFFER_SIZE] {
            for distance_to_end in [0, 1, ALIGNMENT, BUFFER_SIZE, 10 * BUFFER_SIZE + 1] {
                let reader = AlignedReader::new(
                    DirectFile::new(content(num_bytes)),
                    (num_bytes + distance_to_end) as u64,
                    BUFFER_SIZE,
                );
                assert!(
                    read_all(reader.unwrap()).is_empty(),
                    "Something is read {distance_to_end} bytes behind the end of {num_bytes} bytes"
                );
            }
        }
    }

    #[test]
    fn reads_in_pieces_of_any_size() {
        let content = content(3 * BUFFER_SIZE + 1234);
        for piece_size in [1, 7, ALIGNMENT, BUFFER_SIZE - 1, BUFFER_SIZE, 100_000] {
            let mut reader =
                AlignedReader::new(DirectFile::new(content.clone()), 123, BUFFER_SIZE).unwrap();
            let mut result = vec![];
            let mut piece = vec![0; piece_size];
            loop {
                let num_read = reader.read(&mut piece).unwrap();
                if num_read == 0 {
                    break;
                }
                result.extend_from_slice(&piece[..num_read]);
            }
            assert!(
                content[123..] == result,
                "Content isn't read correctly in pieces of {piece_size} bytes"
            );
            // And it stays at its end
            assert_eq!(0, reader.read(&mut piece).unwrap());
        }
    }

    #[test]
    fn reads_with_any_buffer_size() {
        let content = content(100_000);
        for buffer_size in [ALIGNMENT, 3 * ALIGNMENT, 16 * ALIGNMENT, 64 * ALIGNMENT] {
            for position in [0, 1, 50_000] {
                let reader = AlignedReader::new(
                    DirectFile::new(content.clone()),
                    position as u64,
                    buffer_size,
                );
                assert!(
                    content[position..] == read_all(reader.unwrap()),
                    "Content isn't read correctly from position {position} with a buffer of \
                     {buffer_size} bytes"
                );
            }
        }
    }

    #[test]
    fn reads_the_content_when_reads_return_less_than_they_were_asked_for() {
        let content = content(5 * BUFFER_SIZE + 1234);
        let mut file = DirectFile::new(content.clone());
        file.max_read_size = ALIGNMENT;

        let reader = AlignedReader::new(file, 5000, BUFFER_SIZE).unwrap();

        assert!(content[5000..] == read_all(reader));
    }

    #[test]
    fn returns_errors_and_can_go_on_after_them() {
        let content = content(3 * BUFFER_SIZE);
        let mut file = DirectFile::new(content.clone());
        file.failing_reads = vec![0, 2];
        let mut reader = AlignedReader::new(file, 100, BUFFER_SIZE).unwrap();

        let mut result = vec![];
        let mut piece = vec![0; 1000];
        let mut num_errors = 0;
        loop {
            match reader.read(&mut piece) {
                Ok(0) => break,
                Ok(num_read) => result.extend_from_slice(&piece[..num_read]),
                Err(err) => {
                    assert_eq!("Simulated failure", err.to_string());
                    num_errors += 1;
                }
            }
        }

        assert_eq!(2, num_errors);
        assert!(content[100..] == result);
    }

    #[test]
    #[should_panic(expected = "The buffer size has to be a multiple of 4096")]
    fn buffer_size_has_to_be_aligned() {
        let _ = AlignedReader::new(DirectFile::new(vec![]), 0, 5000);
    }
}
