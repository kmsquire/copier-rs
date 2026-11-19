//! `copier` command-line entry point.

fn main() {
    std::process::exit(copier::cli::run(std::env::args_os()));
}
