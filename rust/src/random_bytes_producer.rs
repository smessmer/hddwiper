use anyhow::Result;
use rand::RngCore;

use crate::producer::{new_producer, Producer};

pub fn new_random_bytes_producer(
    block_size: usize,
    buffer_num_blocks: usize,
    num_workers: usize,
) -> Result<impl Producer<Vec<u8>>> {
    let mut rng = None;
    Ok(new_producer(num_workers, buffer_num_blocks, move || {
        if rng.is_none() {
            // Initialize random generator on first run of the worker so that each worker gets a different seed
            rng = Some(crate::random::secure_rng()?);
        }

        // TODO Can we avoid the zero initialization of the buffer?
        let mut buffer = vec![0; block_size];
        rng.as_mut().unwrap().fill_bytes(buffer.as_mut());
        Ok(buffer)
    }))
}
