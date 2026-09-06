use micp_api::run;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("micp-api: {err}");
        std::process::exit(1);
    }
}
