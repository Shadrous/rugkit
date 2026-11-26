//! Binary entry point for `cargo-rugkit` (`cargo rugkit dev`).

use std::process::ExitCode;

fn main() -> ExitCode {
    match rugkit_cli::parse_args(std::env::args_os()) {
        Ok(cli) => rugkit_cli::run_cli(cli),
        Err(err) => {
            let _ = err.print();
            ExitCode::from(err.exit_code() as u8)
        }
    }
}
