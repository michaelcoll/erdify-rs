#[tokio::main]
async fn main() {
    let args = clap::Parser::parse();

    match erdify_rs::run(args).await {
        Ok(outcome) => std::process::exit(outcome.exit_code()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
