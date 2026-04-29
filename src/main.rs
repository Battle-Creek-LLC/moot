use clap::Parser;

use moot::cli::Cli;
use moot::{error::Error, logging};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    logging::init(cli.json, cli.verbose);

    let json = cli.json;
    let exit_code = match cli.dispatch().await {
        Ok(()) => 0,
        Err(err) => {
            report(&err, json);
            err.exit_code()
        }
    };

    std::process::exit(exit_code);
}

fn report(err: &Error, json: bool) {
    if json {
        let payload = serde_json::json!({
            "error": {
                "code": err.code_str(),
                "message": err.to_string(),
            }
        });
        eprintln!("{payload}");
    } else {
        eprintln!("error: {err}");
    }
}
