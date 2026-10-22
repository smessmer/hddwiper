use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};

use crate::aligned_buffer::{AlignedBuffer, ALIGNMENT};

/// Writes to a writer that only accepts aligned writes, see [ALIGNMENT]. To do that, it collects
/// the data it is given in a buffer, and writes whole buffers to that writer.
///
/// Data that doesn't fill a buffer yet stays in the buffer. [Write::flush] writes it anyway, but
/// that only works if the writer accepts a write that isn't a whole number of sectors.
pub struct AlignedWriter<W> {
    inner: W,
    buffer: AlignedBuffer,

    /// The number of bytes at the beginning of the buffer that contain data
    num_filled: usize,

    /// How many of those bytes were written to `inner` already. This is only different from
    /// zero while the buffer is being written, and after that failed before it was finished.
    num_written: usize,
}

impl<W: Read + Write + Seek> AlignedWriter<W> {
    /// Creates a writer that writes to `inner` from `position` on. It writes `buffer_size` bytes
    /// at a time, which has to be a multiple of [ALIGNMENT].
    ///
    /// Writes have to begin at the beginning of a sector. If `position` is within a sector, the
    /// part of that sector in front of `position` is read here and written again with the
    /// first write, so that it stays as it is. That is the only case in which `inner` is read,
    /// see [Self::has_to_read].
    pub fn new(mut inner: W, position: u64, buffer_size: usize) -> io::Result<Self> {
        let mut buffer = AlignedBuffer::new(buffer_size);

        let num_bytes_to_keep = (position % ALIGNMENT as u64) as usize;
        let sector_start = position - num_bytes_to_keep as u64;
        inner.seek(SeekFrom::Start(sector_start))?;
        if num_bytes_to_keep > 0 {
            // This reads less than the sector if `inner` ends within it or before it. Then
            // there is less to keep, and the rest stays zeroes, which is also what a file gets
            // where it is written behind its end.
            loop {
                match inner.read(&mut buffer[..ALIGNMENT]) {
                    Ok(_) => break,
                    Err(err) if err.kind() == ErrorKind::Interrupted => {}
                    Err(err) => return Err(err),
                }
            }
            inner.seek(SeekFrom::Start(sector_start))?;
        }

        Ok(Self {
            inner,
            buffer,
            num_filled: num_bytes_to_keep,
            num_written: 0,
        })
    }

    /// Whether [Self::new] has to read from `inner` to be able to write from `position` on
    pub fn has_to_read(position: u64) -> bool {
        position % ALIGNMENT as u64 != 0
    }
}

impl<W: Write> AlignedWriter<W> {
    /// Writes the data in the buffer to `inner`. If that fails, the data that wasn't written
    /// stays in the buffer, and the next call goes on with it.
    fn write_buffer(&mut self) -> io::Result<()> {
        // At first, we try to write everything at once
        let mut max_write_size = usize::MAX;
        while self.num_written < self.num_filled {
            let end = self
                .num_filled
                .min(self.num_written.saturating_add(max_write_size));
            match self.inner.write(&self.buffer[self.num_written..end]) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(num_bytes) => self.num_written += num_bytes,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                // A file system that is nearly full can refuse a write as a whole, although a
                // part of it would still fit. Sector by sector, all that fits gets written.
                Err(err) if err.kind() == ErrorKind::StorageFull && max_write_size > ALIGNMENT => {
                    max_write_size = ALIGNMENT;
                }
                Err(err) => return Err(err),
            }
        }
        self.num_filled = 0;
        self.num_written = 0;
        Ok(())
    }
}

impl<W: Write> Write for AlignedWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        // A full buffer is only written when more data arrives. If that fails, we haven't
        // taken any of the new data yet, which is what an error of this function has to mean.
        if self.num_filled == self.buffer.len() {
            self.write_buffer()?;
        }
        let num_bytes = data.len().min(self.buffer.len() - self.num_filled);
        self.buffer[self.num_filled..][..num_bytes].copy_from_slice(&data[..num_bytes]);
        self.num_filled += num_bytes;
        Ok(num_bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_buffer()?;
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const BUFFER_SIZE: usize = 2 * ALIGNMENT;

    /// Behaves like a file that was opened for direct reads and writes, so it only accepts
    /// those that are aligned to its sectors
    struct DirectFile {
        sector_size: usize,
        content: Vec<u8>,
        position: usize,

        /// The file can't get larger than this. For a device, that is its size. For a file, it
        /// is where its file system is full.
        max_size: usize,

        /// Whether a write that doesn't fit as a whole is refused as a whole. Otherwise, the
        /// part of it that fits is written.
        refuses_writes_that_dont_fit: bool,

        /// No write takes more bytes than this, no matter how many it is given
        max_write_size: usize,

        /// The writes with these numbers fail, counted from zero
        failing_writes: Vec<usize>,
        num_writes: usize,

        num_reads: usize,
    }

    impl DirectFile {
        /// A file with `content` that can grow up to `max_size`
        fn new(sector_size: usize, content: Vec<u8>, max_size: usize) -> Self {
            assert_eq!(0, max_size % sector_size);
            Self {
                sector_size,
                content,
                position: 0,
                max_size,
                refuses_writes_that_dont_fit: false,
                max_write_size: usize::MAX,
                failing_writes: vec![],
                num_writes: 0,
                num_reads: 0,
            }
        }

        /// A device with `content`, which is also its size
        fn device(sector_size: usize, content: Vec<u8>) -> Self {
            let size = content.len();
            Self::new(sector_size, content, size)
        }

        fn assert_is_aligned(&self, what: &str, data: &[u8]) {
            assert_eq!(
                0,
                self.position % self.sector_size,
                "The {what} doesn't begin at an aligned position"
            );
            assert_eq!(
                0,
                data.len() % self.sector_size,
                "The {what} isn't for an aligned number of bytes"
            );
            assert_eq!(
                0,
                data.as_ptr() as usize % self.sector_size,
                "The {what} doesn't use aligned memory"
            );
        }
    }

    impl Write for DirectFile {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let write_number = self.num_writes;
            self.num_writes += 1;
            if self.failing_writes.contains(&write_number) {
                return Err(io::Error::other("Simulated failure"));
            }

            self.assert_is_aligned("write", data);

            let space = self.max_size.saturating_sub(self.position);
            if space == 0 || (self.refuses_writes_that_dont_fit && data.len() > space) {
                return Err(ErrorKind::StorageFull.into());
            }
            let num_bytes = data.len().min(space).min(self.max_write_size);
            let end = self.position + num_bytes;
            if self.content.len() < end {
                // If the write begins behind the end, this also puts zeroes in between
                self.content.resize(end, 0);
            }
            self.content[self.position..end].copy_from_slice(&data[..num_bytes]);
            self.position = end;
            Ok(num_bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for DirectFile {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            self.num_reads += 1;
            self.assert_is_aligned("read", dest);

            let remaining = &self.content[self.position.min(self.content.len())..];
            let num_bytes = dest.len().min(remaining.len());
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

    /// Data that is different for each `kind`, to tell apart who wrote what
    fn data(kind: u8, num_bytes: usize) -> Vec<u8> {
        (0..num_bytes)
            .map(|i| (i % 251) as u8 ^ kind.wrapping_mul(37))
            .collect()
    }

    /// Writes `data` to `writer`, which has to become full before all of it is written. That
    /// is what a wipe does, with data that never ends.
    fn write_until_full(writer: &mut impl Write, data: &[u8]) {
        let err = writer.write_all(data).unwrap_err();
        assert_eq!(ErrorKind::StorageFull, err.kind());
    }

    #[test]
    fn writes_until_the_device_is_full() {
        // Devices that end at the end of a buffer, and somewhere within one
        for (sector_size, device_size) in [
            (512, 5 * BUFFER_SIZE),
            (512, 5 * BUFFER_SIZE + 512),
            (512, 5 * BUFFER_SIZE - 512),
            (512, 512),
            (4096, 5 * BUFFER_SIZE),
            (4096, 5 * BUFFER_SIZE + 4096),
            (4096, 4096),
            (4096, 0),
        ] {
            let new_data = data(1, device_size + 2 * BUFFER_SIZE);
            let mut device = DirectFile::device(sector_size, data(0, device_size));

            let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
            write_until_full(&mut writer, &new_data);

            assert!(
                new_data[..device_size] == device.content,
                "Wrong data on a device of {device_size} bytes with sectors of {sector_size} bytes"
            );
        }
    }

    #[test]
    fn writes_from_the_position() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        let old_data = data(0, DEVICE_SIZE);
        let new_data = data(1, DEVICE_SIZE + 2 * BUFFER_SIZE);

        for sector_size in [512, 4096] {
            for position in [
                1,
                ALIGNMENT - 1,
                ALIGNMENT,
                ALIGNMENT + 1,
                BUFFER_SIZE - 1,
                BUFFER_SIZE,
                BUFFER_SIZE + 1,
                3 * BUFFER_SIZE + 567,
                // In the last sector
                DEVICE_SIZE - 100,
                DEVICE_SIZE - 1,
            ] {
                let mut device = DirectFile::device(sector_size, old_data.clone());

                let mut writer =
                    AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &new_data);

                // What was in front of the position is still there, also in the sector that
                // the position is in
                assert!(
                    old_data[..position] == device.content[..position],
                    "Data in front of position {position} was changed"
                );
                assert!(
                    new_data[..DEVICE_SIZE - position] == device.content[position..],
                    "Wrong data from position {position} on"
                );
            }
        }
    }

    #[test]
    fn only_reads_if_the_position_is_within_a_sector() {
        for (position, has_to_read) in [
            (0, false),
            (1, true),
            (ALIGNMENT - 1, true),
            (ALIGNMENT, false),
            (ALIGNMENT + 1, true),
            (3 * BUFFER_SIZE, false),
            (3 * BUFFER_SIZE + 567, true),
        ] {
            assert_eq!(
                has_to_read,
                AlignedWriter::<DirectFile>::has_to_read(position as u64)
            );

            let mut device = DirectFile::device(512, data(0, 5 * BUFFER_SIZE));
            let mut writer = AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
            write_until_full(&mut writer, &data(1, 7 * BUFFER_SIZE));
            assert_eq!(
                has_to_read,
                device.num_reads > 0,
                "Wrong for position {position}"
            );
        }
    }

    #[test]
    fn writes_behind_the_end_of_a_file() {
        const MAX_SIZE: usize = 5 * BUFFER_SIZE;
        const POSITION: usize = 10_000;
        let new_data = data(1, MAX_SIZE + 2 * BUFFER_SIZE);

        // The file is empty, ends before the sector of the position, and ends within it
        for file_size in [0, 5000, 9000] {
            let old_data = data(0, file_size);
            let mut file = DirectFile::new(4096, old_data.clone(), MAX_SIZE);

            let mut writer = AlignedWriter::new(&mut file, POSITION as u64, BUFFER_SIZE).unwrap();
            write_until_full(&mut writer, &new_data);

            // Between the old end of the file and the position, there are zeroes now
            let mut expected = old_data;
            expected.resize(POSITION, 0);
            expected.extend_from_slice(&new_data[..MAX_SIZE - POSITION]);
            assert!(
                expected == file.content,
                "Wrong data in a file that had {file_size} bytes"
            );
        }
    }

    #[test]
    fn fills_a_file_system_that_refuses_writes_that_dont_fit() {
        // The last buffer doesn't fit as a whole, in none of these
        for max_size in [
            3 * BUFFER_SIZE + ALIGNMENT,
            3 * BUFFER_SIZE - ALIGNMENT,
            ALIGNMENT,
        ] {
            let new_data = data(1, max_size + 2 * BUFFER_SIZE);
            let mut file = DirectFile::new(4096, vec![], max_size);
            file.refuses_writes_that_dont_fit = true;

            let mut writer = AlignedWriter::new(&mut file, 0, BUFFER_SIZE).unwrap();
            write_until_full(&mut writer, &new_data);

            assert!(
                new_data[..max_size] == file.content,
                "A file system with space for {max_size} bytes wasn't filled"
            );
        }
    }

    #[test]
    fn writes_the_data_when_writes_take_less_than_they_were_given() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        let new_data = data(1, DEVICE_SIZE + 2 * BUFFER_SIZE);
        let mut device = DirectFile::device(512, data(0, DEVICE_SIZE));
        device.max_write_size = 512;

        let mut writer = AlignedWriter::new(&mut device, 5000, BUFFER_SIZE).unwrap();
        write_until_full(&mut writer, &new_data);

        assert!(data(0, 5000) == device.content[..5000]);
        assert!(new_data[..DEVICE_SIZE - 5000] == device.content[5000..]);
    }

    #[test]
    fn returns_errors_and_can_go_on_after_them() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        let new_data = data(1, DEVICE_SIZE + 2 * BUFFER_SIZE);
        let mut device = DirectFile::device(4096, data(0, DEVICE_SIZE));
        // It also takes less than it is given, so that it fails in the middle of a buffer
        device.max_write_size = 4096;
        device.failing_writes = vec![0, 1, 3, 7];

        let mut writer = AlignedWriter::new(&mut device, 100, BUFFER_SIZE).unwrap();
        let mut remaining = &new_data[..];
        let mut num_errors = 0;
        loop {
            match writer.write(remaining) {
                Ok(num_bytes) => remaining = &remaining[num_bytes..],
                Err(err) if err.kind() == ErrorKind::StorageFull => break,
                // The data wasn't taken, so we offer the same data again
                Err(err) => {
                    assert_eq!("Simulated failure", err.to_string());
                    num_errors += 1;
                }
            }
        }

        assert_eq!(4, num_errors);
        assert!(data(0, 100) == device.content[..100]);
        assert!(new_data[..DEVICE_SIZE - 100] == device.content[100..]);
    }

    #[test]
    fn keeps_data_that_doesnt_fill_a_buffer_until_it_is_flushed() {
        let new_data = data(1, BUFFER_SIZE);
        let mut file = DirectFile::new(4096, vec![], 5 * BUFFER_SIZE);

        let mut writer = AlignedWriter::new(&mut file, 0, BUFFER_SIZE).unwrap();
        writer.write_all(&new_data).unwrap();
        // Not even the full buffer is written, because that only happens when more data arrives
        assert!(writer.inner.content.is_empty());

        writer.flush().unwrap();
        assert!(new_data == writer.inner.content);

        // Flushing again doesn't write anything again
        writer.flush().unwrap();
        assert!(new_data == writer.inner.content);

        // And writing goes on behind what was flushed
        writer.write_all(&data(2, ALIGNMENT)).unwrap();
        writer.flush().unwrap();
        assert!(new_data == file.content[..new_data.len()]);
        assert!(data(2, ALIGNMENT) == file.content[new_data.len()..]);
    }

    #[test]
    fn flushes_any_amount_of_data_to_a_writer_that_doesnt_need_alignment() {
        for num_bytes in [
            0,
            1,
            1234,
            BUFFER_SIZE - 1,
            BUFFER_SIZE,
            3 * BUFFER_SIZE + 1234,
        ] {
            let old_data = data(0, 777);
            let new_data = data(1, num_bytes);
            let mut file = Cursor::new(old_data.clone());

            let mut writer = AlignedWriter::new(&mut file, 777, BUFFER_SIZE).unwrap();
            writer.write_all(&new_data).unwrap();
            writer.flush().unwrap();

            let content = file.into_inner();
            assert!(old_data == content[..777]);
            assert!(
                new_data == content[777..],
                "Wrong data after writing {num_bytes} bytes"
            );
        }
    }
}
