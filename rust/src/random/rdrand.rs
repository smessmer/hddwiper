use rand::RngCore;
use rdrand::RdRand;
use zeroize::Zeroize;

// rdrand_or_zeroes returns a random generator based on RDRAND if that instruction
// is available. Otherwise, it just outputs zeroes. This is secure because
// we only use it in an xor composite with other random generators.
pub fn rdrand_or_zeroes() -> impl RngCore + Clone {
    match RdRand::new() {
        Ok(rdrand) => RngOrZeroes(Some(rdrand)),
        Err(err) => {
            println!("Warning: Not able to use RDRAND random generator. Generated keys might be less random. Error message: {}", err);
            RngOrZeroes(None)
        }
    }
}

// RngOrZeroes is a random generator that either generates random values
// based on the underlying Some(rng), or - if the underlying generator
// is None, produces a series of zeroes.
// This is used so we're able to build composites with random generators
// that aren't available on all platforms. This is secure as long as it
// is in a composite with other non-zero random generators.
#[derive(Clone)]
struct RngOrZeroes<R: RngCore>(Option<R>);
impl<R: RngCore> RngCore for RngOrZeroes<R> {
    fn next_u32(&mut self) -> u32 {
        if let Some(rng) = &mut self.0 {
            rng.next_u32()
        } else {
            0
        }
    }

    fn next_u64(&mut self) -> u64 {
        if let Some(rng) = &mut self.0 {
            rng.next_u64()
        } else {
            0
        }
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        if let Some(rng) = &mut self.0 {
            rng.fill_bytes(dest)
        } else {
            Zeroize::zeroize(dest);
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        if let Some(rng) = &mut self.0 {
            rng.try_fill_bytes(dest)
        } else {
            Zeroize::zeroize(dest);
            Ok(())
        }
    }
}
