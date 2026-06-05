#[tokio::main]
async fn main() -> miette::Result<()> {
    keel_lang::run().await
}
