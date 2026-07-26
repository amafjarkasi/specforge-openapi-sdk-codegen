//! Example Rust consumer of a specforge-generated Petstore SDK.
//!
//! ```bash
//! ./scripts/generate-examples.sh
//! cd examples/petstore-rust && cargo run
//! ```

use petstore_example_sdk::api;
use petstore_example_sdk::retry::RetryOptions;
use petstore_example_sdk::Client;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let base = std::env::var("PETSTORE_URL")
        .unwrap_or_else(|_| "https://petstore3.swagger.io/api/v3".into());

    let client = Client::builder()
        .base_url(&base)
        .timeout(Duration::from_secs(10))
        .max_concurrent(4)
        .retry(RetryOptions::default())
        .build()
        .expect("client");

    match api::list_pets(&client, Some(5)).await {
        Ok(pets) => {
            println!("fetched {} pets from {base}", pets.len());
            for p in pets.iter().take(3) {
                println!("- {} (id={})", p.name, p.id);
            }
        }
        Err(e) => {
            eprintln!("list_pets failed (is the server up?): {e}");
            std::process::exit(1);
        }
    }
}
