//! `helio`, built on its own (`cargo build -p helio-cli`): the command line
//! of `lib.rs`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    helio_cli::main(&args)
}
