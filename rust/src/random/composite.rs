use rand::RngCore;

#[derive(Clone)]
pub struct CompositeRng<Rng1: RngCore + Clone, Rng2: RngCore + Clone> {
    rng1: Rng1,
    rng2: Rng2,
}

impl<Rng1: RngCore + Clone, Rng2: RngCore + Clone> CompositeRng<Rng1, Rng2> {
    pub fn new(rng1: Rng1, rng2: Rng2) -> Self {
        Self { rng1, rng2 }
    }
}

impl<Rng1: RngCore + Clone, Rng2: RngCore + Clone> RngCore for CompositeRng<Rng1, Rng2> {
    fn next_u32(&mut self) -> u32 {
        self.rng1.next_u32() ^ self.rng2.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng1.next_u64() ^ self.rng2.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng1.fill_bytes(dest);
        let mut buffer = vec![0; dest.len()];
        self.rng2.fill_bytes(&mut buffer);
        for i in 0..dest.len() {
            dest[i] ^= buffer[i];
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.rng1.try_fill_bytes(dest)?;
        let mut buffer = vec![0; dest.len()];
        self.rng2.try_fill_bytes(&mut buffer)?;
        for i in 0..dest.len() {
            dest[i] ^= buffer[i];
        }
        Ok(())
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
