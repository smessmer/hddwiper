use crate::byte_stream::SyncByteStream;
use anyhow::Result;

#[derive(Clone)]
pub struct CompositeRng<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> {
    rng1: Rng1,
    rng2: Rng2,
}

impl<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> CompositeRng<Rng1, Rng2> {
    pub fn new(rng1: Rng1, rng2: Rng2) -> Self {
        Self { rng1, rng2 }
    }
}

impl<Rng1: SyncByteStream + Send, Rng2: SyncByteStream + Send> SyncByteStream for CompositeRng<Rng1, Rng2> {
    fn blocking_read(&mut self, dest: &mut [u8]) -> Result<()> {
        let mut rng1_data = vec![0; dest.len()];
        self.rng1.blocking_read(&mut rng1_data)?;
        self.rng2.blocking_read(dest)?;
        _apply_xor(dest, &rng1_data);
        Ok(())
    }
}

fn _apply_xor(dest: &mut [u8], source: &[u8]) {
    assert_eq!(dest.len(), source.len());
    for i in 0..dest.len() {
        dest[i] ^= source[i];
    }
}

#[macro_export]
macro_rules! composite_rng {
    ($rng1:expr, $rng2:expr) => {
        crate::random::composite::CompositeRng::new($rng1, $rng2)
    };
    ($rng1:expr, $rng2:expr, $($tail:expr),+) => {
        crate::random::composite::CompositeRng::new($rng1, crate::composite_rng!($rng2, $($tail),+))
    };
}
