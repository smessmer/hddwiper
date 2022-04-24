use anyhow::Result;

use crate::byte_stream::SyncByteStream;
use crate::producer::{new_producer, Producer};

pub fn new_byte_stream_producer<S: 'static + SyncByteStream + Send>(
    block_size: usize,
    buffer_num_blocks: usize,
    num_workers: usize,
    make_byte_stream: impl Fn() -> Result<S>,
) -> Result<impl Producer<Vec<u8>>> {
    new_producer(num_workers, buffer_num_blocks, move || {
        let mut byte_stream = make_byte_stream()?;
        Ok(move || {
            // TODO Can we avoid the zero initialization of the buffer?
            let mut buffer = vec![0; block_size];
            byte_stream.blocking_read(buffer.as_mut()).unwrap();
            Ok(buffer)
        })
    })
}