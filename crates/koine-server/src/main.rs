//! Koiné server binary: composition root wiring adapters to the application core.

mod dev_loop;
mod runtime;
mod serve;
mod sinks;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str);
    if let Some("dev-loop") = command {
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
    } else if let Some("serve") = command {
        match serve::run().await {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(msg) => {
                eprintln!("serve failed: {msg}");
                std::process::ExitCode::FAILURE
            }
        }
    } else {
        println!("koine-server 0.1.0 — commands: dev-loop, serve");
        std::process::ExitCode::SUCCESS
    }
}
