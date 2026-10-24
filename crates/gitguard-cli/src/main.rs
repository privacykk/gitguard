use std::io::{self, Write};

fn main() {
    let mut stdin = io::stdin().lock();
    let mut stderr = io::stderr().lock();
    let exit_code = gitguard_cli::run(std::env::args(), &mut stdin, &mut stderr);
    if stderr.flush().is_err() && exit_code == 0 {
        std::process::exit(gitguard_core::CliExitCode::IoError.as_i32());
    }
    std::process::exit(exit_code);
}
