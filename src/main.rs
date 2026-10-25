#[tokio::main]
async fn main() {
    // MBW_ADDR (default 127.0.0.1:8787) and optional MBW_PASSWORD
    let addr = std::env::var("MBW_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".into());
    if let Err(e) = moviebox_web::serve(addr, true).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
