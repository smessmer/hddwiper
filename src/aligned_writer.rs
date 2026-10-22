use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};

use crate::aligned_buffer::{AlignedBuffer, ALIGNMENT};

/// Writes to a writer that only accepts aligned writes, see [ALIGNMENT]. To do that, it collects
/// the data it is given in a buffer, and writes whole buffers to that writer.
///
/// Data that is aligned already and wouldn't fit into the buffer anyway isn't collected. It is
/// written as it is, which saves copying it into the buffer. See
/// [Self::num_bytes_to_write_directly].
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

    /// No write to `inner` is for more bytes than this. There only is such a limit once `inner`
    /// refused a write because it had no space for it, see [Self::write_to_inner].
    max_write_size: usize,
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
        assert!(
            buffer_size > 0 && buffer_size % ALIGNMENT == 0,
            "The buffer size has to be a multiple of {ALIGNMENT}, but is {buffer_size}"
        );
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
            max_write_size: usize::MAX,
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
        while self.num_written < self.num_filled {
            self.num_written += Self::write_to_inner(
                &mut self.inner,
                &mut self.max_write_size,
                &self.buffer[self.num_written..self.num_filled],
            )?;
        }
        self.num_filled = 0;
        self.num_written = 0;
        Ok(())
    }

    /// Writes the beginning of `data` to `inner` and returns how many bytes it wrote. `data`
    /// has to be aligned.
    fn write_to_inner(inner: &mut W, max_write_size: &mut usize, data: &[u8]) -> io::Result<usize> {
        loop {
            // At first, we try to write everything at once
            let num_bytes = data.len().min(*max_write_size);
            match inner.write(&data[..num_bytes]) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                // A file system that is nearly full can refuse a write as a whole, although a
                // part of it would still fit. Sector by sector, all that fits gets written.
                // From here on that is how everything is written, because what a refused write
                // was for comes back in pieces, and each piece would get refused again together
                // with all that follows it.
                Err(err) if err.kind() == ErrorKind::StorageFull && num_bytes > ALIGNMENT => {
                    *max_write_size = ALIGNMENT;
                }
                result => return result,
            }
        }
    }

    /// How many bytes from the beginning of `data` can be written to `inner` as they are,
    /// without copying them into the buffer first.
    ///
    /// For that, the memory of `data` has to be aligned. And the data has to begin a sector, so
    /// the data in the buffer, which is written before it, has to end at the end of one. Data
    /// that is smaller than the buffer is collected even then, because many small writes are
    /// slower than a few large ones.
    fn num_bytes_to_write_directly(&self, data: &[u8]) -> usize {
        let is_aligned =
            data.as_ptr() as usize % ALIGNMENT == 0 && self.num_filled % ALIGNMENT == 0;
        let num_bytes_in_whole_sectors = data.len() - data.len() % ALIGNMENT;
        if is_aligned && num_bytes_in_whole_sectors >= self.buffer.len() {
            num_bytes_in_whole_sectors
        } else {
            0
        }
    }
}

impl<W: Write> Write for AlignedWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let num_bytes_direct = self.num_bytes_to_write_directly(data);
        if num_bytes_direct > 0 {
            // What is in the buffer comes first. If one of the two writes fails, we haven't
            // taken any of the new data yet, which is what an error of this function has to
            // mean.
            self.write_buffer()?;
            return Self::write_to_inner(
                &mut self.inner,
                &mut self.max_write_size,
                &data[..num_bytes_direct],
            );
        }

        // A full buffer is only written when more data arrives. If that fails, we haven't
        // taken any of the new data yet either.
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
    use std::ops::{Deref, Range};

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

        /// The writes with these numbers get interrupted before they wrote anything
        interrupted_writes: Vec<usize>,
        num_writes: usize,

        /// How many writes were refused because there was no space for them
        num_refused_writes: usize,

        /// Where in memory each write took the data from that it wrote
        written_memory: Vec<Range<usize>>,

        /// The next reads fail with these errors, one after the other
        read_errors: Vec<ErrorKind>,
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
                interrupted_writes: vec![],
                num_writes: 0,
                num_refused_writes: 0,
                written_memory: vec![],
                read_errors: vec![],
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

        /// Whether any write took its data from the memory of `data`
        fn has_written_from_memory_of(&self, data: &[u8]) -> bool {
            self.written_memory.iter().any(|memory| {
                memory.start < memory_of(data).end && memory_of(data).start < memory.end
            })
        }
    }

    impl Write for DirectFile {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let write_number = self.num_writes;
            self.num_writes += 1;
            if self.failing_writes.contains(&write_number) {
                return Err(io::Error::other("Simulated failure"));
            }
            if self.interrupted_writes.contains(&write_number) {
                return Err(ErrorKind::Interrupted.into());
            }

            self.assert_is_aligned("write", data);

            let space = self.max_size.saturating_sub(self.position);
            if space == 0 || (self.refuses_writes_that_dont_fit && data.len() > space) {
                self.num_refused_writes += 1;
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
            self.written_memory.push(memory_of(&data[..num_bytes]));
            Ok(num_bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for DirectFile {
        fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
            self.num_reads += 1;
            if !self.read_errors.is_empty() {
                return Err(self.read_errors.remove(0).into());
            }
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

    /// The addresses of the memory that `data` is in
    fn memory_of(data: &[u8]) -> Range<usize> {
        let address = data.as_ptr() as usize;
        address..address + data.len()
    }

    /// Where the data is in memory that a test gives to the writer. The writer treats aligned
    /// data in another way than data that isn't, and what gets written has to be the same.
    #[derive(Clone, Copy, Debug)]
    enum Memory {
        Aligned,
        Unaligned,
    }

    impl Memory {
        const BOTH: [Memory; 2] = [Memory::Aligned, Memory::Unaligned];

        /// A copy of `data` in memory of this kind
        fn copy_of(self, data: &[u8]) -> DataInMemory {
            match self {
                Memory::Aligned => DataInMemory::new(data, 0),
                // Nothing that follows this is aligned either, until the next sector
                Memory::Unaligned => DataInMemory::new(data, 1),
            }
        }
    }

    /// Data at an address that is `offset` bytes behind an aligned address
    struct DataInMemory {
        memory: AlignedBuffer,
        offset: usize,
    }

    impl DataInMemory {
        fn new(data: &[u8], offset: usize) -> Self {
            let mut memory = AlignedBuffer::new(offset + data.len());
            memory[offset..].copy_from_slice(data);
            Self { memory, offset }
        }
    }

    impl Deref for DataInMemory {
        type Target = [u8];

        fn deref(&self) -> &[u8] {
            &self.memory[self.offset..]
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
            for memory in Memory::BOTH {
                let new_data = memory.copy_of(&data(1, device_size + 2 * BUFFER_SIZE));
                let mut device = DirectFile::device(sector_size, data(0, device_size));

                let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &new_data);

                assert!(
                    new_data[..device_size] == device.content,
                    "Wrong data on a device of {device_size} bytes with sectors of {sector_size} \
                     bytes, written from {memory:?} memory"
                );
            }
        }
    }

    #[test]
    fn writes_from_the_position() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        let old_data = data(0, DEVICE_SIZE);

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
                for memory in Memory::BOTH {
                    let new_data = memory.copy_of(&data(1, DEVICE_SIZE + 2 * BUFFER_SIZE));
                    let mut device = DirectFile::device(sector_size, old_data.clone());

                    let mut writer =
                        AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
                    write_until_full(&mut writer, &new_data);

                    // What was in front of the position is still there, also in the sector that
                    // the position is in
                    assert!(
                        old_data[..position] == device.content[..position],
                        "Data in front of position {position} was changed, with data from \
                         {memory:?} memory"
                    );
                    assert!(
                        new_data[..DEVICE_SIZE - position] == device.content[position..],
                        "Wrong data from position {position} on, written from {memory:?} memory"
                    );
                }
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

            for memory in Memory::BOTH {
                let mut device = DirectFile::device(512, data(0, 5 * BUFFER_SIZE));
                let mut writer =
                    AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &memory.copy_of(&data(1, 7 * BUFFER_SIZE)));
                assert_eq!(
                    has_to_read,
                    device.num_reads > 0,
                    "Wrong for position {position}"
                );
            }
        }
    }

    #[test]
    fn reads_the_beginning_of_the_sector_again_after_it_was_interrupted() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        let old_data = data(0, DEVICE_SIZE);
        let new_data = Memory::Unaligned.copy_of(&data(1, DEVICE_SIZE + 2 * BUFFER_SIZE));
        let mut device = DirectFile::device(4096, old_data.clone());
        device.read_errors = vec![ErrorKind::Interrupted, ErrorKind::Interrupted];

        let mut writer = AlignedWriter::new(&mut device, 5000, BUFFER_SIZE).unwrap();
        write_until_full(&mut writer, &new_data);

        assert_eq!(3, device.num_reads);
        assert!(old_data[..5000] == device.content[..5000]);
        assert!(new_data[..DEVICE_SIZE - 5000] == device.content[5000..]);
    }

    #[test]
    fn fails_if_the_beginning_of_the_sector_cant_be_read() {
        let old_data = data(0, 5 * BUFFER_SIZE);
        let mut device = DirectFile::device(4096, old_data.clone());
        device.read_errors = vec![ErrorKind::PermissionDenied];

        let result = AlignedWriter::new(&mut device, 5000, BUFFER_SIZE);

        assert_eq!(
            ErrorKind::PermissionDenied,
            result.map(|_| ()).unwrap_err().kind()
        );
        assert!(old_data == device.content);
    }

    #[test]
    fn fails_if_a_write_takes_nothing() {
        for memory in Memory::BOTH {
            let old_data = data(0, 5 * BUFFER_SIZE);
            let mut device = DirectFile::device(4096, old_data.clone());
            device.max_write_size = 0;

            let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
            let new_data = memory.copy_of(&data(1, 2 * BUFFER_SIZE));
            let err = writer.write_all(&new_data).unwrap_err();

            // Otherwise it would try to write forever
            assert_eq!(ErrorKind::WriteZero, err.kind());
            assert!(old_data == device.content);
        }
    }

    #[test]
    fn writes_behind_the_end_of_a_file() {
        const MAX_SIZE: usize = 5 * BUFFER_SIZE;
        const POSITION: usize = 10_000;

        // The file is empty, ends before the sector of the position, and ends within it
        for file_size in [0, 5000, 9000] {
            for memory in Memory::BOTH {
                let new_data = memory.copy_of(&data(1, MAX_SIZE + 2 * BUFFER_SIZE));
                let old_data = data(0, file_size);
                let mut file = DirectFile::new(4096, old_data.clone(), MAX_SIZE);

                let mut writer =
                    AlignedWriter::new(&mut file, POSITION as u64, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &new_data);

                // Between the old end of the file and the position, there are zeroes now
                let mut expected = old_data;
                expected.resize(POSITION, 0);
                expected.extend_from_slice(&new_data[..MAX_SIZE - POSITION]);
                assert!(
                    expected == file.content,
                    "Wrong data in a file that had {file_size} bytes, written from {memory:?} \
                     memory"
                );
            }
        }
    }

    #[test]
    fn fills_a_file_system_that_refuses_writes_that_dont_fit() {
        // The last write doesn't fit as a whole, in none of these
        for max_size in [
            3 * BUFFER_SIZE + ALIGNMENT,
            3 * BUFFER_SIZE - ALIGNMENT,
            ALIGNMENT,
        ] {
            for memory in Memory::BOTH {
                let new_data = memory.copy_of(&data(1, max_size + 2 * BUFFER_SIZE));
                let mut file = DirectFile::new(4096, vec![], max_size);
                file.refuses_writes_that_dont_fit = true;

                let mut writer = AlignedWriter::new(&mut file, 0, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &new_data);

                assert!(
                    new_data[..max_size] == file.content,
                    "A file system with space for {max_size} bytes wasn't filled from {memory:?} \
                     memory"
                );
                // One write that didn't fit as a whole, and the one for the sector that didn't
                // fit at all. After the first one, nothing but single sectors was tried anymore.
                assert_eq!(2, file.num_refused_writes);
            }
        }
    }

    #[test]
    fn writes_the_data_when_writes_take_less_than_they_were_given() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;

        for position in [0, 5000] {
            for memory in Memory::BOTH {
                let new_data = memory.copy_of(&data(1, DEVICE_SIZE + 2 * BUFFER_SIZE));
                let mut device = DirectFile::device(512, data(0, DEVICE_SIZE));
                device.max_write_size = 512;

                let mut writer =
                    AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
                write_until_full(&mut writer, &new_data);

                assert!(data(0, position) == device.content[..position]);
                assert!(
                    new_data[..DEVICE_SIZE - position] == device.content[position..],
                    "Wrong data from position {position} on, written from {memory:?} memory"
                );
            }
        }
    }

    #[test]
    fn writes_that_get_interrupted_are_tried_again() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;

        for memory in Memory::BOTH {
            let new_data = memory.copy_of(&data(1, 2 * BUFFER_SIZE));
            let mut device = DirectFile::device(4096, data(0, DEVICE_SIZE));
            device.max_write_size = 4096;
            device.interrupted_writes = vec![0, 1, 3, 6];

            // Without write_all() and the like, which would try again by themselves
            let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
            let mut remaining = &new_data[..];
            while !remaining.is_empty() {
                let num_bytes = writer.write(remaining).unwrap();
                remaining = &remaining[num_bytes..];
            }
            writer.flush().unwrap();

            assert_eq!(4 + 4, device.num_writes);
            assert!(
                new_data[..] == device.content[..2 * BUFFER_SIZE],
                "Wrong data, written from {memory:?} memory"
            );
        }
    }

    #[test]
    fn returns_errors_and_can_go_on_after_them() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;

        for position in [0, 100] {
            for memory in Memory::BOTH {
                let new_data = memory.copy_of(&data(1, DEVICE_SIZE + 2 * BUFFER_SIZE));
                let mut device = DirectFile::device(4096, data(0, DEVICE_SIZE));
                // It also takes less than it is given, so that it fails in the middle of a
                // buffer
                device.max_write_size = 4096;
                device.failing_writes = vec![0, 1, 3, 7];

                let mut writer =
                    AlignedWriter::new(&mut device, position as u64, BUFFER_SIZE).unwrap();
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
                assert!(data(0, position) == device.content[..position]);
                assert!(
                    new_data[..DEVICE_SIZE - position] == device.content[position..],
                    "Wrong data from position {position} on, written from {memory:?} memory"
                );
            }
        }
    }

    #[test]
    fn keeps_data_that_doesnt_fill_a_buffer_until_it_is_flushed() {
        let new_data = Memory::Unaligned.copy_of(&data(1, BUFFER_SIZE));
        let mut file = DirectFile::new(4096, vec![], 5 * BUFFER_SIZE);

        let mut writer = AlignedWriter::new(&mut file, 0, BUFFER_SIZE).unwrap();
        writer.write_all(&new_data).unwrap();
        // Not even the full buffer is written, because that only happens when more data arrives
        assert!(writer.inner.content.is_empty());

        writer.flush().unwrap();
        assert!(new_data[..] == writer.inner.content);

        // Flushing again doesn't write anything again
        writer.flush().unwrap();
        assert!(new_data[..] == writer.inner.content);

        // And writing goes on behind what was flushed
        writer.write_all(&data(2, ALIGNMENT)).unwrap();
        writer.flush().unwrap();
        assert!(new_data[..] == file.content[..new_data.len()]);
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
            for memory in Memory::BOTH {
                let old_data = data(0, 777);
                let new_data = memory.copy_of(&data(1, num_bytes));
                let mut file = Cursor::new(old_data.clone());

                let mut writer = AlignedWriter::new(&mut file, 777, BUFFER_SIZE).unwrap();
                writer.write_all(&new_data).unwrap();
                writer.flush().unwrap();

                let content = file.into_inner();
                assert!(old_data == content[..777]);
                assert!(
                    new_data[..] == content[777..],
                    "Wrong data after writing {num_bytes} bytes from {memory:?} memory"
                );
            }
        }
    }

    #[test]
    fn writes_aligned_data_from_its_own_memory() {
        // As large as the buffer, and larger
        for num_bytes in [BUFFER_SIZE, 5 * BUFFER_SIZE] {
            let new_data = Memory::Aligned.copy_of(&data(1, num_bytes));
            let mut device = DirectFile::device(4096, data(0, 10 * BUFFER_SIZE));

            let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
            writer.write_all(&new_data).unwrap();

            // All of it with one write, which didn't get a copy of the data. And without a
            // flush, because nothing of it is in the buffer.
            assert_eq!(vec![memory_of(&new_data)], device.written_memory);
            assert!(new_data[..] == device.content[..num_bytes]);
        }
    }

    #[test]
    fn writes_the_whole_sectors_of_aligned_data_from_its_own_memory() {
        const NUM_BYTES: usize = 3 * BUFFER_SIZE + 1234;
        let new_data = Memory::Aligned.copy_of(&data(1, NUM_BYTES));
        let more_data = Memory::Aligned.copy_of(&data(2, 2 * BUFFER_SIZE));
        let mut device = DirectFile::device(4096, data(0, 10 * BUFFER_SIZE));

        let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
        writer.write_all(&new_data).unwrap();
        // What follows that begins within a sector. So it is copied, although it is aligned.
        writer.write_all(&more_data).unwrap();

        assert_eq!(
            memory_of(&new_data[..3 * BUFFER_SIZE]),
            device.written_memory[0]
        );
        assert!(!device.has_written_from_memory_of(&new_data[3 * BUFFER_SIZE..]));
        assert!(!device.has_written_from_memory_of(&more_data));
        // What doesn't fill a buffer anymore is still in the buffer
        let num_bytes_written = device.position;
        assert_eq!(5 * BUFFER_SIZE, num_bytes_written);
        assert!(new_data[..] == device.content[..NUM_BYTES]);
        assert!(
            more_data[..num_bytes_written - NUM_BYTES]
                == device.content[NUM_BYTES..num_bytes_written]
        );
    }

    #[test]
    fn collects_aligned_data_that_is_smaller_than_the_buffer() {
        // Each piece is for whole sectors, but for less than a buffer
        let pieces: Vec<DataInMemory> = (0..6)
            .map(|kind| Memory::Aligned.copy_of(&data(kind, ALIGNMENT)))
            .collect();
        let mut device = DirectFile::device(4096, data(100, 10 * BUFFER_SIZE));

        let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
        for piece in &pieces {
            writer.write_all(piece).unwrap();
        }
        writer.flush().unwrap();

        // Many small writes would be slower than these few larger ones
        assert_eq!(3, device.written_memory.len());
        for (index, piece) in pieces.iter().enumerate() {
            assert!(!device.has_written_from_memory_of(piece));
            assert!(piece[..] == device.content[index * ALIGNMENT..][..ALIGNMENT]);
        }
    }

    #[test]
    fn writes_the_data_in_the_buffer_before_aligned_data() {
        // Whole sectors, so that the aligned data that follows them begins a sector
        let first_data = Memory::Unaligned.copy_of(&data(1, ALIGNMENT));
        let second_data = Memory::Aligned.copy_of(&data(2, 3 * BUFFER_SIZE));
        let mut device = DirectFile::device(4096, data(0, 10 * BUFFER_SIZE));

        let mut writer = AlignedWriter::new(&mut device, 0, BUFFER_SIZE).unwrap();
        writer.write_all(&first_data).unwrap();
        writer.write_all(&second_data).unwrap();

        assert_eq!(2, device.written_memory.len());
        assert!(!device.has_written_from_memory_of(&first_data));
        assert_eq!(memory_of(&second_data), device.written_memory[1]);
        assert!(first_data[..] == device.content[..ALIGNMENT]);
        assert!(second_data[..] == device.content[ALIGNMENT..][..3 * BUFFER_SIZE]);
    }

    #[test]
    fn copies_aligned_data_that_begins_within_a_sector() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        const POSITION: usize = 100;
        let old_data = data(0, DEVICE_SIZE);
        let new_data = Memory::Aligned.copy_of(&data(1, DEVICE_SIZE + 2 * BUFFER_SIZE));
        let mut device = DirectFile::device(4096, old_data.clone());

        let mut writer = AlignedWriter::new(&mut device, POSITION as u64, BUFFER_SIZE).unwrap();
        write_until_full(&mut writer, &new_data);

        // Its memory is aligned, but where it goes on the device isn't. A device doesn't accept
        // that, so all of it has to go through the buffer.
        assert!(!device.has_written_from_memory_of(&new_data));
        assert!(old_data[..POSITION] == device.content[..POSITION]);
        assert!(new_data[..DEVICE_SIZE - POSITION] == device.content[POSITION..]);
    }

    #[test]
    fn only_copies_the_beginning_of_data_that_is_aligned_like_its_position() {
        const DEVICE_SIZE: usize = 5 * BUFFER_SIZE;
        const POSITION: usize = 100;
        let old_data = data(0, DEVICE_SIZE);
        // The data is as far behind an aligned address as the position is behind the beginning
        // of its sector. That is how the first block of a wipe is, if the wipe begins within a
        // sector.
        let new_data = DataInMemory::new(&data(1, DEVICE_SIZE - POSITION), POSITION);
        let more_data = Memory::Aligned.copy_of(&data(2, BUFFER_SIZE));
        let mut device = DirectFile::device(4096, old_data.clone());

        let mut writer = AlignedWriter::new(&mut device, POSITION as u64, BUFFER_SIZE).unwrap();
        writer.write_all(&new_data).unwrap();
        write_until_full(&mut writer, &more_data);

        // The first buffer has the beginning of the sector that was there before. Behind that
        // buffer, the data is aligned in both ways.
        let (copied, written_directly) = new_data.split_at(BUFFER_SIZE - POSITION);
        assert!(!device.has_written_from_memory_of(copied));
        assert_eq!(
            vec![memory_of(written_directly)],
            device.written_memory[1..]
        );
        assert!(old_data[..POSITION] == device.content[..POSITION]);
        assert!(new_data[..] == device.content[POSITION..]);
    }

    #[test]
    fn writes_any_mix_of_data() {
        use rand::{rngs::StdRng, Rng, SeedableRng};

        // Each case has its own seed, so that one that fails can be run alone
        for case in 0..300 {
            let mut rng = StdRng::seed_from_u64(case);

            // What refuses writes that don't fit is a file system. Then we only fill it in
            // units of 4096 bytes, so that is what its space has to consist of.
            let refuses_writes_that_dont_fit = rng.gen_bool(0.3);
            let sector_size = if refuses_writes_that_dont_fit || rng.gen_bool(0.5) {
                4096
            } else {
                512
            };
            let max_size = rng.gen_range(0..=12 * BUFFER_SIZE / sector_size) * sector_size;
            let old_data = data(0, rng.gen_range(0..=max_size));
            let position = match rng.gen_range(0..3) {
                0 => 0,
                1 => rng.gen_range(0..=max_size / ALIGNMENT) * ALIGNMENT,
                _ => rng.gen_range(0..=max_size),
            };
            let mut file = DirectFile::new(sector_size, old_data.clone(), max_size);
            file.refuses_writes_that_dont_fit = refuses_writes_that_dont_fit;
            if rng.gen_bool(0.3) {
                file.max_write_size =
                    rng.gen_range(1..=3 * BUFFER_SIZE / sector_size) * sector_size;
            }
            file.interrupted_writes = (0..rng.gen_range(0..4))
                .map(|_| rng.gen_range(0..20))
                .collect();

            // More data than fits, in pieces of all sizes, at all kinds of addresses
            let new_data = data(1, max_size + 3 * BUFFER_SIZE);
            let mut pieces = vec![];
            let mut num_bytes_in_pieces = 0;
            while num_bytes_in_pieces < new_data.len() {
                let max_piece_size = match rng.gen_range(0..3) {
                    0 => 100,
                    1 => BUFFER_SIZE,
                    _ => 4 * BUFFER_SIZE,
                };
                let piece_size = match rng.gen_range(0..3) {
                    0 => rng.gen_range(0..=max_piece_size / ALIGNMENT) * ALIGNMENT,
                    _ => rng.gen_range(0..=max_piece_size),
                }
                .min(new_data.len() - num_bytes_in_pieces);
                let offset = match rng.gen_range(0..4) {
                    0 | 1 => 0,
                    // Aligned like the position that the piece is for
                    2 => (position + num_bytes_in_pieces) % ALIGNMENT,
                    _ => rng.gen_range(0..ALIGNMENT),
                };
                let piece = &new_data[num_bytes_in_pieces..][..piece_size];
                pieces.push(DataInMemory::new(piece, offset));
                num_bytes_in_pieces += piece_size;
            }

            let mut writer = AlignedWriter::new(&mut file, position as u64, BUFFER_SIZE).unwrap();
            let result = pieces.iter().try_for_each(|piece| writer.write_all(piece));

            assert_eq!(
                ErrorKind::StorageFull,
                result.unwrap_err().kind(),
                "Case {case}"
            );
            // In front of the position, there is what was there before, and zeroes where the
            // file ended before the position
            let mut expected = old_data;
            expected.truncate(position);
            if position < max_size {
                expected.resize(position, 0);
                expected.extend_from_slice(&new_data[..max_size - position]);
            }
            assert!(
                expected == file.content[..expected.len()],
                "Case {case}: Wrong data in a file with sectors of {sector_size} bytes and space \
                 for {max_size} bytes, written from position {position} on"
            );
        }
    }

    #[test]
    #[should_panic(expected = "The buffer size has to be a multiple of 4096")]
    fn buffer_size_has_to_be_aligned() {
        let _ = AlignedWriter::new(DirectFile::new(4096, vec![], 0), 0, 5000);
    }

    #[test]
    #[should_panic(expected = "The buffer size has to be a multiple of 4096")]
    fn buffer_size_must_not_be_zero() {
        let _ = AlignedWriter::new(DirectFile::new(4096, vec![], 0), 0, 0);
    }
}
