use miette::Result;

mod cli;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
