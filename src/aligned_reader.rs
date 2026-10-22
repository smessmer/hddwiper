use anyhow::{anyhow, Context, Result};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::aligned_buffer::{AlignedBuffer, ALIGNMENT};

/// Reads from a reader that only accepts aligned reads, see [ALIGNMENT]. To do that, it reads
/// whole buffers from that reader and hands out the data in them, one buffer at a time.
///
/// It reads in a thread of its own, ahead of the data that it handed out. So while somebody
/// looks at the data of one buffer, the next buffers are already being read.
pub struct AlignedReader {
    /// The buffers that the thread has read, in the order of their data. `None` says that the
    /// reader is at its end.
    filled_buffers: Receiver<Result<Option<FilledBuffer>>>,

    /// The way back to the thread for buffers whose data isn't needed anymore
    empty_buffers: Sender<AlignedBuffer>,

    /// The buffer with the data that was handed out last
    current: Option<FilledBuffer>,

    /// Whether all data was handed out
    at_end: bool,
}

/// A buffer that the thread has read into
struct FilledBuffer {
    buffer: AlignedBuffer,

    /// The part of the buffer that has the data to hand out
    data: Range<usize>,
}

impl AlignedReader {
    /// Creates a reader for the content of `inner` from `position` on. It reads `buffer_size`
    /// bytes at a time, which has to be a multiple of [ALIGNMENT], and begins with that at a
    /// multiple of `buffer_size`. So reads are also aligned for devices with larger sectors.
    ///
    /// It has `num_buffers` buffers. One of them is for the data that was handed out last, and
    /// the others are what it reads ahead.
    pub fn new(
        mut inner: impl 'static + Read + Seek + Send,
        position: u64,
        buffer_size: usize,
        num_buffers: usize,
    ) -> io::Result<Self> {
        assert!(
            buffer_size > 0 && buffer_size % ALIGNMENT == 0,
            "The buffer size has to be a multiple of {ALIGNMENT}, but is {buffer_size}"
        );
        assert!(num_buffers > 0, "There has to be a buffer to read into");

        // Reads have to begin at an aligned position, which can be in front of the position
        // that we were asked to read from. The bytes in between get read, but aren't handed out.
        let num_bytes_to_skip = (position % buffer_size as u64) as usize;
        let read_position = position - num_bytes_to_skip as u64;
        inner.seek(SeekFrom::Start(read_position))?;

        let (filled_buffers_of_thread, filled_buffers) = mpsc::channel();
        let (empty_buffers, empty_buffers_of_thread) = mpsc::channel();
        for _ in 0..num_buffers {
            empty_buffers
                .send(AlignedBuffer::new(buffer_size))
                .expect("The other end of the channel is right here");
        }
        thread::spawn(move || {
            read_ahead(
                inner,
                read_position,
                num_bytes_to_skip,
                empty_buffers_of_thread,
                filled_buffers_of_thread,
            )
        });

        Ok(Self {
            filled_buffers,
            empty_buffers,
            current: None,
            at_end: false,
        })
    }

    /// Returns the data that follows the data that the last call returned, or `None` at the
    /// end. This fails if the data couldn't be read. After that, there is no more data to get,
    /// and each further call fails as well.
    pub fn next_data(&mut self) -> Result<Option<&[u8]>> {
        // Nobody needs the data of the last call anymore, so the thread can read into its
        // buffer again
        if let Some(current) = self.current.take() {
            // If the thread is gone, it doesn't need the buffer
            let _ = self.empty_buffers.send(current.buffer);
        }
        if !self.at_end {
            // Before the end, the thread only stops without telling so if reading failed, which
            // it told the call before, or if it crashed
            let next = self.filled_buffers.recv();
            self.current = next.map_err(|_| anyhow!("The thread that reads has stopped"))??;
            self.at_end = self.current.is_none();
        }
        Ok(self
            .current
            .as_ref()
            .map(|current| &current.buffer[current.data.clone()]))
    }
}

/// What the thread of an [AlignedReader] does: It reads `inner` into the buffers it gets from
/// `empty_buffers` and sends them to `filled_buffers`. `position` is where `inner` is at, and
/// the first `num_bytes_to_skip` bytes from there on aren't data to hand out.
fn read_ahead(
    mut inner: impl Read,
    mut position: u64,
    mut num_bytes_to_skip: usize,
    empty_buffers: Receiver<AlignedBuffer>,
    filled_buffers: Sender<Result<Option<FilledBuffer>>>,
) {
    // When the reader is dropped, its ends of the channels are closed. That is when this ends
    // if it didn't get to the end of `inner` before.
    while let Ok(mut buffer) = empty_buffers.recv() {
        let num_read = match read_until_full_or_end(&mut inner, &mut buffer) {
            Ok(num_read) => num_read,
            Err((num_read, err)) => {
                let position = position + num_read as u64;
                let err =
                    Err(err).context(format!("Failed to read the data at position {position}"));
                let _ = filled_buffers.send(err);
                return;
            }
        };
        position += num_read as u64;
        let at_end = num_read < buffer.len();

        let num_skipped = num_bytes_to_skip.min(num_read);
        num_bytes_to_skip -= num_skipped;
        // Only the last buffer can be without data to hand out
        if num_skipped < num_read {
            let filled_buffer = FilledBuffer {
                buffer,
                data: num_skipped..num_read,
            };
            if filled_buffers.send(Ok(Some(filled_buffer))).is_err() {
                return;
            }
        }
        if at_end {
            let _ = filled_buffers.send(Ok(None));
            return;
        }
    }
}

/// Reads until `buffer` is full or `inner` is at its end. Returns the number of bytes read. If
/// reading fails, it returns the error together with the number of bytes read before it.
fn read_until_full_or_end(
    inner: &mut impl Read,
    buffer: &mut [u8],
) -> Result<usize, (usize, io::Error)> {
    let mut num_read = 0;
    while num_read < buffer.len() {
        match inner.read(&mut buffer[num_read..]) {
            Ok(0) => break,
            Ok(num_bytes) => {
                num_read += num_bytes;
                // Only the end of a file makes a read stop in the middle of a sector. We also
                // couldn't go on behind it, because the next read wouldn't begin at an aligned
                // position.
                if num_read % ALIGNMENT != 0 {
                    break;
                }
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err((num_read, err)),
        }
    }
    Ok(num_read)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const BUFFER_SIZE: usize = 2 * ALIGNMENT;

    /// Behaves like a file that was opened for direct reads, so it only accepts aligned reads
    struct DirectFile {
        content: Vec<u8>,
        position: usize,

        /// No read returns more bytes than this, no matter how many it was asked for
        max_read_size: usize,

        /// The reads with these numbers fail, counted from zero
        failing_reads: Vec<usize>,

        /// The reads with these numbers get interrupted before they read anything
        interrupted_reads: Vec<usize>,

        /// How many reads there were. A test can keep a clone of this to see it from outside.
        num_reads: Arc<AtomicUsize>,

        /// Set when this is dropped
        is_dropped: Arc<AtomicBool>,
    }

    impl DirectFile {
        fn new(content: Vec<u8>) -> Self {
            Self {
                content,
                position: 0,
                max_read_size: usize::MAX,
                failing_reads: vec![],
                interrupted_reads: vec![],
                num_reads: Arc::new(AtomicUsize::new(0)),
                is_dropped: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Read for DirectFile {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            let read_number = self.num_reads.fetch_add(1, Ordering::SeqCst);
            if self.failing_reads.contains(&read_number) {
                return Err(io::Error::other("Simulated failure"));
            }
            if self.interrupted_reads.contains(&read_number) {
                return Err(ErrorKind::Interrupted.into());
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

    impl Drop for DirectFile {
        fn drop(&mut self) {
            self.is_dropped.store(true, Ordering::SeqCst);
        }
    }

    fn content(num_bytes: usize) -> Vec<u8> {
        (0..num_bytes).map(|i| (i % 251) as u8).collect()
    }

    /// The pieces of data that `reader` hands out until it is at its end
    fn read_pieces(mut reader: AlignedReader) -> Vec<Vec<u8>> {
        let mut pieces = vec![];
        while let Some(data) = reader.next_data().unwrap() {
            assert!(!data.is_empty(), "A piece without data was handed out");
            pieces.push(data.to_vec());
        }
        // And it stays at its end
        assert!(reader.next_data().unwrap().is_none());
        pieces
    }

    fn read_all(reader: AlignedReader) -> Vec<u8> {
        read_pieces(reader).concat()
    }

    /// Waits until `condition` holds and fails the test if that takes too long
    fn wait_until(condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < deadline, "Timed out waiting for condition");
            std::thread::sleep(Duration::from_millis(1));
        }
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
            let reader = AlignedReader::new(DirectFile::new(content.clone()), 0, BUFFER_SIZE, 3);
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
                3,
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
                    3,
                );
                assert!(
                    read_all(reader.unwrap()).is_empty(),
                    "Something is read {distance_to_end} bytes behind the end of {num_bytes} bytes"
                );
            }
        }
    }

    #[test]
    fn hands_out_the_data_of_one_buffer_at_a_time() {
        let content = content(3 * BUFFER_SIZE + 1234);
        let reader = AlignedReader::new(DirectFile::new(content.clone()), 123, BUFFER_SIZE, 3);

        let pieces = read_pieces(reader.unwrap());

        // The first buffer begins in front of the position, and the last one isn't full
        let sizes: Vec<usize> = pieces.iter().map(|piece| piece.len()).collect();
        assert_eq!(
            vec![BUFFER_SIZE - 123, BUFFER_SIZE, BUFFER_SIZE, 1234],
            sizes
        );
        assert!(content[123..] == pieces.concat());
    }

    #[test]
    fn reads_with_any_buffer_size_and_number_of_buffers() {
        let content = content(100_000);
        for buffer_size in [ALIGNMENT, 3 * ALIGNMENT, 16 * ALIGNMENT, 64 * ALIGNMENT] {
            for num_buffers in [1, 2, 5] {
                for position in [0, 1, 50_000] {
                    let reader = AlignedReader::new(
                        DirectFile::new(content.clone()),
                        position as u64,
                        buffer_size,
                        num_buffers,
                    );
                    assert!(
                        content[position..] == read_all(reader.unwrap()),
                        "Content isn't read correctly from position {position} with \
                         {num_buffers} buffers of {buffer_size} bytes"
                    );
                }
            }
        }
    }

    #[test]
    fn reads_the_content_when_reads_return_less_than_they_were_asked_for() {
        let content = content(5 * BUFFER_SIZE + 1234);
        let mut file = DirectFile::new(content.clone());
        file.max_read_size = ALIGNMENT;

        let reader = AlignedReader::new(file, 5000, BUFFER_SIZE, 3).unwrap();
        let pieces = read_pieces(reader);

        // It still hands out whole buffers
        assert_eq!(BUFFER_SIZE, pieces[1].len());
        assert!(content[5000..] == pieces.concat());
    }

    #[test]
    fn reads_that_get_interrupted_are_tried_again() {
        let content = content(5 * BUFFER_SIZE + 1234);
        let mut file = DirectFile::new(content.clone());
        file.max_read_size = ALIGNMENT;
        file.interrupted_reads = vec![0, 1, 3, 6];

        let reader = AlignedReader::new(file, 100, BUFFER_SIZE, 3).unwrap();

        assert!(content[100..] == read_all(reader));
    }

    #[test]
    fn fails_if_reading_fails() {
        let content = content(5 * BUFFER_SIZE);
        let mut file = DirectFile::new(content.clone());
        // In the middle of the third buffer
        file.max_read_size = ALIGNMENT;
        file.failing_reads = vec![5];
        let mut reader = AlignedReader::new(file, 100, BUFFER_SIZE, 3).unwrap();

        // The data of the buffers before that one is still handed out
        let mut result = vec![];
        let err = loop {
            match reader.next_data() {
                Ok(data) => result.extend_from_slice(data.expect("It ended without failing")),
                Err(err) => break err,
            }
        };

        assert!(content[100..2 * BUFFER_SIZE] == result);
        // It says where reading failed, and why
        assert_eq!(
            format!(
                "Failed to read the data at position {}: Simulated failure",
                2 * BUFFER_SIZE + ALIGNMENT
            ),
            format!("{err:#}")
        );
        // After that, there is nothing more to get
        assert!(reader.next_data().is_err());
        assert!(reader.next_data().is_err());
    }

    #[test]
    fn fails_if_the_position_cant_be_seeked_to() {
        /// A file that can't seek, like a pipe
        struct NoSeek;

        impl Read for NoSeek {
            fn read(&mut self, _dest: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }

        impl Seek for NoSeek {
            fn seek(&mut self, _position: SeekFrom) -> io::Result<u64> {
                Err(ErrorKind::NotSeekable.into())
            }
        }

        let result = AlignedReader::new(NoSeek, 0, BUFFER_SIZE, 3);

        assert_eq!(
            ErrorKind::NotSeekable,
            result.map(|_| ()).unwrap_err().kind()
        );
    }

    #[test]
    fn fails_if_its_thread_crashes() {
        /// A file whose reads panic
        struct Crashing;

        impl Read for Crashing {
            fn read(&mut self, _dest: &mut [u8]) -> io::Result<usize> {
                panic!("Simulated crash");
            }
        }

        impl Seek for Crashing {
            fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
                Ok(if let SeekFrom::Start(position) = position {
                    position
                } else {
                    0
                })
            }
        }

        let mut reader = AlignedReader::new(Crashing, 0, BUFFER_SIZE, 3).unwrap();

        // Otherwise the one who waits for the data would wait forever, or take this for the end
        let err = reader.next_data().unwrap_err();
        assert_eq!("The thread that reads has stopped", err.to_string());
    }

    #[test]
    fn reads_ahead_as_far_as_it_has_buffers() {
        let content = content(10 * BUFFER_SIZE);
        let file = DirectFile::new(content.clone());
        let num_reads = Arc::clone(&file.num_reads);
        let has_num_reads = |expected: usize| {
            wait_until(|| num_reads.load(Ordering::SeqCst) == expected);
            // And it stays at that
            std::thread::sleep(Duration::from_millis(20));
            num_reads.load(Ordering::SeqCst) == expected
        };

        // Nobody has asked for data yet, and all buffers get filled
        let mut reader = AlignedReader::new(file, 0, BUFFER_SIZE, 3).unwrap();
        assert!(has_num_reads(3));

        // The data that is handed out stays in its buffer, so there is none to read into
        assert!(content[..BUFFER_SIZE] == *reader.next_data().unwrap().unwrap());
        assert!(has_num_reads(3));

        // Only when the next data is asked for, the buffer of the data before is free again
        assert!(content[BUFFER_SIZE..2 * BUFFER_SIZE] == *reader.next_data().unwrap().unwrap());
        assert!(has_num_reads(4));
        assert!(content[2 * BUFFER_SIZE..3 * BUFFER_SIZE] == *reader.next_data().unwrap().unwrap());
        assert!(has_num_reads(5));
    }

    #[test]
    fn stops_reading_when_it_is_dropped() {
        let file = DirectFile::new(content(10 * BUFFER_SIZE));
        let num_reads = Arc::clone(&file.num_reads);
        let file_is_dropped = Arc::clone(&file.is_dropped);
        let mut reader = AlignedReader::new(file, 0, BUFFER_SIZE, 3).unwrap();
        reader.next_data().unwrap().unwrap();
        wait_until(|| num_reads.load(Ordering::SeqCst) == 3);

        drop(reader);

        // Its thread ends, which is when the file is dropped, and it didn't read on before
        wait_until(|| file_is_dropped.load(Ordering::SeqCst));
        assert_eq!(3, num_reads.load(Ordering::SeqCst));
    }

    #[test]
    fn its_thread_ends_at_the_end_of_the_content() {
        let file = DirectFile::new(content(2 * BUFFER_SIZE));
        let file_is_dropped = Arc::clone(&file.is_dropped);

        // Although the reader is still there
        let _reader = AlignedReader::new(file, 0, BUFFER_SIZE, 3).unwrap();

        wait_until(|| file_is_dropped.load(Ordering::SeqCst));
    }

    #[test]
    #[should_panic(expected = "The buffer size has to be a multiple of 4096")]
    fn buffer_size_has_to_be_aligned() {
        let _ = AlignedReader::new(DirectFile::new(vec![]), 0, 5000, 3);
    }

    #[test]
    #[should_panic(expected = "There has to be a buffer to read into")]
    fn there_has_to_be_a_buffer() {
        let _ = AlignedReader::new(DirectFile::new(vec![]), 0, BUFFER_SIZE, 0);
    }
}
