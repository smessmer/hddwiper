use std::ops::{Deref, DerefMut, Range};

/// A file that was opened for direct reads or writes only accepts reads and writes that are
/// aligned to the sectors of its device. The position in the file where one begins, the number
/// of bytes it is for and the address of the memory it uses all have to be multiples of the
/// sector size. Sectors commonly have 512 or 4096 bytes, and this is a multiple of both.
pub const ALIGNMENT: usize = 4096;

/// Memory for aligned reads and writes, see [ALIGNMENT]. Its address is aligned. Whether its
/// size is aligned as well is up to the one who creates it.
pub struct AlignedBuffer {
    /// The buffer is a part of this. This has more bytes than the buffer needs, because the
    /// buffer has to begin at an aligned address, and the memory we get usually doesn't.
    memory: Vec<u8>,

    /// The part of `memory` that is the buffer
    range: Range<usize>,
}

impl AlignedBuffer {
    /// Creates a buffer of `size` zeroes
    pub fn new(size: usize) -> Self {
        let memory = vec![0; size + ALIGNMENT - 1];
        let address = memory.as_ptr() as usize;
        let start = address.next_multiple_of(ALIGNMENT) - address;

        Self {
            memory,
            range: start..(start + size),
        }
    }
}

impl Deref for AlignedBuffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.memory[self.range.clone()]
    }
}

impl DerefMut for AlignedBuffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.memory[self.range.clone()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_the_size_and_is_aligned() {
        // Several of them, because one alone could be aligned by chance. Their sizes are and
        // aren't whole sectors.
        const SIZES: [usize; 10] = [
            ALIGNMENT,
            2 * ALIGNMENT,
            3 * ALIGNMENT,
            16 * ALIGNMENT,
            1,
            1000,
            ALIGNMENT - 1,
            ALIGNMENT + 1,
            64 * ALIGNMENT,
            100_000,
        ];
        let buffers: Vec<AlignedBuffer> = SIZES.into_iter().map(AlignedBuffer::new).collect();

        for (buffer, size) in buffers.iter().zip(SIZES) {
            assert_eq!(size, buffer.len());
            assert_eq!(0, buffer.as_ptr() as usize % ALIGNMENT);
        }
    }

    #[test]
    fn can_be_empty() {
        let buffer = AlignedBuffer::new(0);

        assert!(buffer.is_empty());
        assert_eq!(0, buffer.as_ptr() as usize % ALIGNMENT);
    }

    #[test]
    fn consists_of_zeroes_and_can_be_changed() {
        let mut buffer = AlignedBuffer::new(2 * ALIGNMENT);
        assert!(buffer.iter().all(|byte| *byte == 0));

        buffer[0] = 1;
        buffer[2 * ALIGNMENT - 1] = 2;

        assert_eq!(1, buffer[0]);
        assert_eq!(2, buffer[2 * ALIGNMENT - 1]);
        assert!(buffer[1..2 * ALIGNMENT - 1].iter().all(|byte| *byte == 0));
    }
}
