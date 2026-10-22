use std::ops::{Deref, DerefMut, Range};

/// A file that was opened for direct reads or writes only accepts reads and writes that are
/// aligned to the sectors of its device. The position in the file where one begins, the number
/// of bytes it is for and the address of the memory it uses all have to be multiples of the
/// sector size. Sectors commonly have 512 or 4096 bytes, and this is a multiple of both.
pub const ALIGNMENT: usize = 4096;

/// Memory usually consists of pages of 4096 bytes, but it can also consist of huge pages of
/// this many bytes. This is their size on x86-64, and on ARM with the usual page size.
const HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;

/// Memory for aligned reads and writes, see [ALIGNMENT]. Its address is aligned. Whether its
/// size is aligned as well is up to the one who creates it.
///
/// A buffer of at least [HUGE_PAGE_SIZE] bytes asks the system for huge pages. For each direct
/// read or write, the system has to look up and lock every page of the memory that is read into
/// or written from. With pages of 4096 bytes, that takes most of the time the system needs for
/// a read or write, and it is what limits the speed on very fast devices. Huge pages are 512
/// times fewer.
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
        // Only memory that begins where a huge page begins can consist of huge pages
        let alignment = if size >= HUGE_PAGE_SIZE {
            HUGE_PAGE_SIZE
        } else {
            ALIGNMENT
        };
        let memory = vec![0; size + alignment - 1];
        let address = memory.as_ptr() as usize;
        let start = address.next_multiple_of(alignment) - address;

        let buffer = Self {
            memory,
            range: start..(start + size),
        };
        // This has to happen before the buffer is used. Memory only gets its pages when it is
        // used for the first time, and this is what decides which pages it gets.
        buffer.ask_for_huge_pages();
        buffer
    }

    /// Asks the system to use huge pages for the part of the buffer that whole huge pages fit
    /// into. Whether it does is up to the system. It doesn't if it was configured to never use
    /// them, if it doesn't know them at all, or if it has no memory left that huge pages fit
    /// into. The buffer then consists of pages of the usual size, like the rest of it.
    #[cfg(target_os = "linux")]
    fn ask_for_huge_pages(&self) {
        let num_bytes = self.len() - self.len() % HUGE_PAGE_SIZE;
        if num_bytes == 0 {
            return;
        }

        // SAFETY: The range is a part of the buffer, which is memory that belongs to us, and
        // it begins at a page since it begins at a multiple of HUGE_PAGE_SIZE. MADV_HUGEPAGE
        // only changes which pages the memory gets, not what it contains.
        //
        // What the system answers doesn't matter. A buffer that it refuses huge pages for is
        // as good as it would have been without asking.
        unsafe {
            libc::madvise(
                self.as_ptr() as *mut libc::c_void,
                num_bytes,
                libc::MADV_HUGEPAGE,
            );
        }
    }

    /// Only Linux can be asked for huge pages this way
    #[cfg(not(target_os = "linux"))]
    fn ask_for_huge_pages(&self) {}
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
    fn large_buffer_has_the_size_and_begins_at_a_huge_page() {
        // Their sizes are and aren't whole huge pages
        const SIZES: [usize; 6] = [
            HUGE_PAGE_SIZE,
            HUGE_PAGE_SIZE + 1,
            HUGE_PAGE_SIZE + ALIGNMENT,
            2 * HUGE_PAGE_SIZE - 1,
            3 * HUGE_PAGE_SIZE,
            5 * HUGE_PAGE_SIZE + 1000,
        ];
        let buffers: Vec<AlignedBuffer> = SIZES.into_iter().map(AlignedBuffer::new).collect();

        for (buffer, size) in buffers.iter().zip(SIZES) {
            assert_eq!(size, buffer.len());
            assert_eq!(0, buffer.as_ptr() as usize % HUGE_PAGE_SIZE);
        }
    }

    #[test]
    fn buffer_that_is_too_small_for_a_huge_page_has_the_size_and_is_aligned() {
        let buffer = AlignedBuffer::new(HUGE_PAGE_SIZE - 1);

        assert_eq!(HUGE_PAGE_SIZE - 1, buffer.len());
        assert_eq!(0, buffer.as_ptr() as usize % ALIGNMENT);
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

    #[test]
    fn large_buffer_consists_of_zeroes_and_can_be_changed() {
        const SIZE: usize = 2 * HUGE_PAGE_SIZE + 1000;
        let mut buffer = AlignedBuffer::new(SIZE);
        assert!(buffer.iter().all(|byte| *byte == 0));

        buffer[0] = 1;
        buffer[SIZE - 1] = 2;

        assert_eq!(1, buffer[0]);
        assert_eq!(2, buffer[SIZE - 1]);
        assert!(buffer[1..SIZE - 1].iter().all(|byte| *byte == 0));
    }

    /// What the system says about a part of the memory of this process
    #[cfg(target_os = "linux")]
    struct Mapping {
        addresses: Range<usize>,

        /// Whether the system was asked to use huge pages for it
        asked_for_huge_pages: bool,

        num_bytes_in_huge_pages: usize,
    }

    /// The parts of the memory of this process that `addresses` are in
    #[cfg(target_os = "linux")]
    fn mappings_of(addresses: Range<usize>) -> Vec<Mapping> {
        let mut mappings: Vec<Mapping> = Vec::new();
        for line in std::fs::read_to_string("/proc/self/smaps").unwrap().lines() {
            let (name, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            // A mapping begins with a line like "7f5c00000000-7f5c00a00000 rw-p 00000000 00:00 0",
            // and the lines that follow are what is known about it
            let range = name.split_once('-').and_then(|(start, end)| {
                let start = usize::from_str_radix(start, 16).ok()?;
                let end = usize::from_str_radix(end, 16).ok()?;
                Some(start..end)
            });
            if let Some(range) = range {
                mappings.push(Mapping {
                    addresses: range,
                    asked_for_huge_pages: false,
                    num_bytes_in_huge_pages: 0,
                });
                continue;
            }
            let mapping = mappings.last_mut().expect("smaps begins with a mapping");
            match name {
                "AnonHugePages:" => {
                    let num_kb: usize = value.trim().trim_end_matches("kB").trim().parse().unwrap();
                    mapping.num_bytes_in_huge_pages = num_kb * 1024;
                }
                "VmFlags:" => {
                    mapping.asked_for_huge_pages =
                        value.split_whitespace().any(|flag| flag == "hg");
                }
                _ => {}
            }
        }
        mappings.retain(|mapping| {
            mapping.addresses.start < addresses.end && addresses.start < mapping.addresses.end
        });
        mappings
    }

    /// What the system does with huge pages: "always", "madvise" or "never". None if it doesn't
    /// know huge pages at all.
    #[cfg(target_os = "linux")]
    fn huge_page_setting() -> Option<String> {
        let settings =
            std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled").ok()?;
        // The one that is in effect is in brackets, like in "always [madvise] never"
        let (_, rest) = settings.split_once('[')?;
        let (setting, _) = rest.split_once(']')?;
        Some(setting.to_string())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn asks_for_huge_pages_for_all_that_whole_huge_pages_fit_into() {
        // A system that doesn't know huge pages refuses to be asked for them
        let can_be_asked = huge_page_setting().is_some();

        for (size, num_huge_pages) in [
            (HUGE_PAGE_SIZE, 1),
            (5 * HUGE_PAGE_SIZE, 5),
            (3 * HUGE_PAGE_SIZE + ALIGNMENT, 3),
            (4 * HUGE_PAGE_SIZE - 1, 3),
        ] {
            let buffer = AlignedBuffer::new(size);
            let start = buffer.as_ptr() as usize;
            let end = start + num_huge_pages * HUGE_PAGE_SIZE;

            let mappings = mappings_of(start..end);

            // All of it was asked for, without a gap
            let mut next_address = start;
            for mapping in &mappings {
                assert!(
                    mapping.asked_for_huge_pages || !can_be_asked,
                    "Didn't ask for huge pages for the addresses {:x?} of a buffer of {size} \
                     bytes at {start:x}",
                    mapping.addresses
                );
                assert!(mapping.addresses.start <= next_address);
                next_address = mapping.addresses.end;
            }
            assert!(next_address >= end);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn large_buffer_consists_of_huge_pages_once_it_is_used() {
        const NUM_HUGE_PAGES: usize = 8;
        let mut buffer = AlignedBuffer::new(NUM_HUGE_PAGES * HUGE_PAGE_SIZE);
        let start = buffer.as_ptr() as usize;

        // Memory only gets its pages when it is used
        buffer.fill(1);

        let num_bytes_in_huge_pages: usize = mappings_of(start..start + buffer.len())
            .iter()
            .map(|mapping| mapping.num_bytes_in_huge_pages)
            .sum();
        // A system that is short of memory may not have a huge page for each of them, but one
        // that has none at all for a buffer that asked for them in time most likely wasn't
        // asked the right way. Unless it never uses huge pages, then there are none to see.
        let uses_huge_pages = matches!(huge_page_setting().as_deref(), Some("always" | "madvise"));
        assert!(
            num_bytes_in_huge_pages >= HUGE_PAGE_SIZE || !uses_huge_pages,
            "Only {num_bytes_in_huge_pages} bytes of the buffer are in huge pages"
        );
    }
}
