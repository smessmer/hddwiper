mod cancellation_token;
mod producer;
mod random;
mod random_bytes_producer;

use producer::Producer;

#[tokio::main]
async fn main() {
    let random_producer = random_bytes_producer::new_random_bytes_producer(1024, 1024, 5).unwrap();
    loop {
        random_producer.get_product().await.unwrap();
    }
}
