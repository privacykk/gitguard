//! 二进制入口：只负责准备 IO 与退出进程，实际逻辑在 `gitguard_cli::run` 里。

use std::io::{self, Write};

fn main() {
    // Hook 需要把 stdin 完整读完；先加锁，避免与其他输入源交错。
    let mut stdin = io::stdin().lock();
    let mut stderr = io::stderr().lock();
    let exit_code = gitguard_cli::run(std::env::args(), &mut stdin, &mut stderr);
    // 如果 stderr 写失败但判定是「放行」（0），改用 I/O 错误码退出，
    // 避免「提示没写出去却让 push 继续」这种不可观测的放行。
    if stderr.flush().is_err() && exit_code == 0 {
        std::process::exit(gitguard_core::CliExitCode::IoError.as_i32());
    }
    std::process::exit(exit_code);
}
