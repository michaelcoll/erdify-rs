#[tokio::main]
async fn main() {
    let args = clap::Parser::parse();

    match erdify_rs::run(args).await {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
