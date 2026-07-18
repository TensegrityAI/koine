//! Koiné server binary: composition root wiring adapters to the application core.

mod dev_loop;
mod runtime;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some("dev-loop") = args.get(1).map(String::as_str) {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://koine:koine@localhost:5432/koine".into());
        match dev_loop::run(&url).await {
            Ok(()) => {
                println!("dev-loop: all jobs terminal — stack exercised end-to-end");
                std::process::ExitCode::SUCCESS
            }
            Err(msg) => {
                eprintln!("dev-loop failed: {msg}");
                std::process::ExitCode::FAILURE
            }
        }
    } else {
        println!("koine-server 0.1.0 — commands: dev-loop");
        std::process::ExitCode::SUCCESS
    }
}
