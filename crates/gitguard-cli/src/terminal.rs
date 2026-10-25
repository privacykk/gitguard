//! 终端内的推送确认界面。
//!
//! `pre-push` Hook 的 stdin 已经被 Git 用来传 ref 列表，读完就没有了，
//! 因此这里不能从 stdin 读用户输入，而是单独打开控制终端（`/dev/tty`）。
//! 也正因为如此，「能不能打开控制终端」天然区分了两类调用方：
//!
//! - 终端里执行 `git push`：可以交互，直接在这里展示并确认；
//! - SourceTree / VS Code / CI 等图形客户端：没有控制终端，只能降级到桌面端确认。
//!
//! 安全约定：
//! - 展示内容来自仓库（ref 名、提交标题），写进终端前必须过滤控制字符，
//!   否则仓库内容可以借助终端转义序列干扰界面；
//! - remote URL 可能带用户名和密码，展示前做脱敏；
//! - 超时、EOF、读失败一律按「取消」处理，绝不默认放行。

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::time::Duration;

use gitguard_audit::{redact_url, sanitize_text};
use gitguard_core::{CommitInfo, GitGuardError, RefUpdate, Result};

/// 每页展示的行数。提交很多时用户可以翻页查看，不会一次刷满终端。
const PAGE_LINES: usize = 30;

/// 单行提交标题的最大显示宽度（按字符计），超出部分截断并加省略号。
const MAX_SUBJECT_CHARS: usize = 72;

/// 头部「标签 + 值」两列里标签列的宽度（按字符计），用来让值左对齐。
const LABEL_WIDTH: usize = 11;

/// 终端配色开关：负责给关键内容着色，同时过滤所有来自仓库的文本。
///
/// 安全约定：ref 名、作者、提交标题、remote URL 等外部文本一律先经过这里的方法，
/// 它们在包裹颜色码之前会统一做 [`sanitize_text`]，因此仓库内容无法夹带 ANSI 转义
/// 序列伪造界面；输出里的转义序列只由本模块自己写入。
///
/// 当 `NO_COLOR` 存在或 `TERM=dumb` 时退化为纯文本，保证重定向、录制与
/// 不支持颜色的终端里不会出现颜色码。
#[derive(Debug, Clone, Copy)]
struct Paint {
    enabled: bool,
}

impl Paint {
    /// 按当前环境决定是否输出颜色。
    fn for_terminal() -> Self {
        let disabled = std::env::var_os("NO_COLOR").is_some()
            || std::env::var_os("TERM").is_some_and(|term| term == "dumb");
        Self { enabled: !disabled }
    }

    /// 测试用：不带颜色的纯文本。
    #[cfg(test)]
    fn plain() -> Self {
        Self { enabled: false }
    }

    /// 用 SGR 参数（如 `1;36`）包装文本；关闭颜色时只做过滤。
    fn style(&self, code: &str, text: &str) -> String {
        let text = sanitize_text(text);
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text
        }
    }

    /// 标题：加粗青色。
    fn title(&self, text: &str) -> String {
        self.style("1;36", text)
    }

    /// 需要用户重点看的内容（仓库路径、分支名、提交标题、键位）：加粗。
    fn strong(&self, text: &str) -> String {
        self.style("1", text)
    }

    /// 次要信息（标签、时间、URL）：暗色。
    fn dim(&self, text: &str) -> String {
        self.style("2", text)
    }

    /// 作者等辅助信息：青色。
    fn accent(&self, text: &str) -> String {
        self.style("36", text)
    }

    /// 新建 ref：绿色。
    fn ok(&self, text: &str) -> String {
        self.style("32", text)
    }

    /// 更新 ref 与短 SHA：黄色。
    fn warn(&self, text: &str) -> String {
        self.style("33", text)
    }

    /// 删除 ref 与历史改写（非快进）：红色。
    fn danger(&self, text: &str) -> String {
        self.style("31", text)
    }
}

/// 一个待推送 ref 及其提交列表，用于终端展示。
pub(crate) struct PreviewRef {
    pub update: RefUpdate,
    pub commits: Vec<CommitInfo>,
    /// 本次更新会丢弃远端已有的提交（非快进），仅用于风险提示。
    pub rewrites_history: bool,
}

/// 终端确认所需的全部展示数据。
pub(crate) struct Preview {
    pub repository_path: String,
    pub remote_name: String,
    pub remote_url: String,
    pub refs: Vec<PreviewRef>,
}

/// 用户在终端里的决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// 用户输入了 `yes`。
    Confirmed,
    /// 用户输入了 `no`、读到 EOF，或终端读写出错。
    Declined,
    /// 在配置的超时时间内没有任何决定。
    TimedOut,
}

/// 在控制终端上展示请求并等待用户决定。
///
/// - 没有控制终端时返回 `Ok(None)`（图形客户端、CI、被重定向等），
///   调用方应当降级到桌面端确认；
/// - 有终端时先调用 `build_preview` 取展示数据：收集失败直接返回错误，
///   不会出现「列不出要推什么却让用户盲签」的情况；
/// - 返回 `Some` 时三种决定都必须被当成本次 push 的最终结果，
///   不能因为超时或取消而放行。
pub(crate) fn confirm_with<F>(build_preview: F, timeout: Duration) -> Result<Option<Decision>>
where
    F: FnOnce() -> Result<Preview>,
{
    // 先看 Git 的输出去向：只有 stdout/stderr 里至少一个是终端，才说明
    // 有人真的看着这个终端。图形客户端、CI 与后台脚本会把两者都重定向成
    // 管道，即使它们碰巧继承了控制终端，也不应该弹出一个没人看的确认界面。
    if !has_interactive_output() {
        return Ok(None);
    }
    let Some(tty) = open_control_terminal() else {
        return Ok(None);
    };
    // 只有在确实存在控制终端时才去执行 git log，无终端场景不做无用功。
    let preview = build_preview()?;
    // 克隆一个句柄用于读取，原句柄留给写入。
    let reader = tty.try_clone().map_err(|error| {
        GitGuardError::Io(format!("could not duplicate the terminal handle: {error}"))
    })?;
    let paint = Paint::for_terminal();
    let lines = render(&preview, &paint);

    // 读取用户输入是阻塞的，放到单独线程里，主线程用超时等待。
    // 超时后进程很快就会退出，阻塞中的线程会随进程一起结束。
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let decision = interact(BufReader::new(reader), tty, &lines, &paint);
        let _ = sender.send(decision);
    });

    match receiver.recv_timeout(timeout) {
        Ok(decision) => Ok(Some(decision)),
        Err(_) => Ok(Some(Decision::TimedOut)),
    }
}

/// 当前进程是否有可交互的控制终端。
///
/// CLI 在记录 `request_created` 审计事件之前调用：只有确定能在终端里确认时，
/// 才把本次请求记为终端通道；否则记为桌面端通道。判断条件与 [`confirm_with`]
/// 完全一致（stdout/stderr 至少一个是终端，且能打开 `/dev/tty`）。
pub(crate) fn is_available() -> bool {
    has_interactive_output() && open_control_terminal().is_some()
}

/// stdout 或 stderr 是否至少有一个连着终端。
///
/// 在终端里执行 `git push` 时两者通常都是终端；SourceTree、VS Code、CI
/// 会把它们重定向成管道。stdin 不能参与判断：Hook 的 stdin 始终是 Git
/// 写入 ref 列表的管道。
fn has_interactive_output() -> bool {
    use std::io::IsTerminal;

    std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
}

/// 打开当前进程的控制终端。
///
/// Unix 下是 `/dev/tty`：它指向进程所属的控制终端，与 stdin/stdout 是否被
/// 重定向无关。图形客户端启动的 Git 通常没有控制终端，这里会失败，从而
/// 自动降级到桌面端确认。
#[cfg(unix)]
fn open_control_terminal() -> Option<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()
}

/// Windows 需要另外实现（`CONIN$` / `CONOUT$`），当前版本一律返回 `None`，
/// 调用方因此降级到桌面端确认，仍然保持 fail-closed。
#[cfg(not(unix))]
fn open_control_terminal() -> Option<File> {
    None
}

/// 分页展示内容并逐行读取用户输入。
///
/// 支持的命令：`y`/`yes` 确认，`n`/`no` 取消，`m` 下一页，`b` 上一页。
/// 无法识别的输入只会重新渲染当前页，不会给出任何默认决定。
fn interact<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    lines: &[String],
    paint: &Paint,
) -> Decision {
    let pages = lines.len().div_ceil(PAGE_LINES).max(1);
    let mut page = 0;

    loop {
        let start = page * PAGE_LINES;
        let end = (start + PAGE_LINES).min(lines.len());
        let mut output = String::new();
        for line in &lines[start..end] {
            output.push_str(line);
            output.push('\n');
        }

        output.push_str(&hint_line(paint, page, pages));
        output.push_str(&format!("\n{} ", paint.strong(">")));

        if writer.write_all(output.as_bytes()).is_err() || writer.flush().is_err() {
            return Decision::Declined;
        }

        let mut answer = String::new();
        match reader.read_line(&mut answer) {
            // EOF 表示对话结束（例如终端被关闭）：按取消处理。
            Ok(0) | Err(_) => return Decision::Declined,
            Ok(_) => {}
        }

        match answer.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return Decision::Confirmed,
            "n" | "no" => return Decision::Declined,
            "m" | "next" if page + 1 < pages => page += 1,
            "b" | "back" if page > 0 => page -= 1,
            _ => {}
        }
    }
}

/// 分页提示行：页码只在多页时出现，键位提示始终可见。
fn hint_line(paint: &Paint, page: usize, pages: usize) -> String {
    let mut hints = String::new();
    if pages > 1 {
        hints.push_str(&paint.dim(&format!("[page {}/{}]  ", page + 1, pages)));
    }
    hints.push_str(&format!(
        "{} {}  {} {}",
        paint.strong("[y]"),
        "confirm",
        paint.strong("[n]"),
        "cancel"
    ));
    if page + 1 < pages {
        hints.push_str(&format!("  {} {}", paint.strong("[m]"), "next"));
    }
    if page > 0 {
        hints.push_str(&format!("  {} {}", paint.strong("[b]"), "previous"));
    }
    hints
}

/// 把一次推送请求渲染成待展示的文本行。
///
/// 这里只做「文本 → 行」的转换，不写终端，方便单独测试；
/// `paint` 同时负责给关键内容着色，并过滤所有来自仓库的文本。
fn render(preview: &Preview, paint: &Paint) -> Vec<String> {
    let total_commits: usize = preview.refs.iter().map(|entry| entry.commits.len()).sum();
    let mut lines = vec![
        paint.title("GitGuard · push confirmation"),
        String::new(),
        label_row(paint, "repository", &paint.strong(&preview.repository_path)),
        label_row(
            paint,
            "remote",
            &format!(
                "{}  {}",
                paint.strong(&preview.remote_name),
                paint.dim(&format!("({})", redact_url(&preview.remote_url)))
            ),
        ),
        label_row(
            paint,
            "changes",
            &format!(
                "{}, {}",
                count_label(preview.refs.len(), "ref", "refs"),
                count_label(total_commits, "commit", "commits")
            ),
        ),
        String::new(),
    ];

    for (index, entry) in preview.refs.iter().enumerate() {
        let update = &entry.update;
        // 目标以远端 ref 为准：删除分支时 local_ref 是 Git 的 `(delete)` 占位符。
        let target = short_ref(&update.remote_ref);
        let source = short_ref(&update.local_ref);
        let name = if update.is_deletion() || source == target {
            target.to_string()
        } else {
            // `git push origin local:remote` 这类改名推送，两边都要让用户看清。
            format!("{source} -> {target}")
        };

        let status = if update.is_deletion() {
            paint.danger("(delete)")
        } else if update.is_creation() {
            paint.ok("(create)")
        } else if entry.rewrites_history {
            // 非快进更新（--force / --force-with-lease / +refspec）会丢弃远端提交。
            paint.danger("(rewrite)")
        } else {
            paint.warn("(update)")
        };

        let mut header = String::new();
        // 只有一个 ref 时省掉序号，避免最常见的场景里多读一段无信息量的前缀。
        if preview.refs.len() > 1 {
            header.push_str(&paint.dim(&format!("[{}/{}] ", index + 1, preview.refs.len())));
        }
        header.push_str(&format!(
            "{} {} {}",
            paint.dim(&format!("{}:", ref_kind(&update.remote_ref))),
            paint.strong(&name),
            status
        ));
        lines.push(header);

        // 短 SHA 先补齐再拼接，作者与提交时间才能在所有 commit 之间对齐。
        let sha_width = entry
            .commits
            .iter()
            .map(|commit| commit.short_sha.chars().count())
            .max()
            .unwrap_or(0)
            .max(7);
        let indent = " ".repeat(4 + sha_width + 2);

        for commit in &entry.commits {
            lines.push(format!(
                "    {}  {}",
                paint.warn(&format!("{:<width$}", commit.short_sha, width = sha_width)),
                paint.strong(&truncate(&commit.subject)),
            ));
            lines.push(format!(
                "{indent}{} {} {}",
                paint.accent(&commit.author),
                paint.dim("·"),
                paint.dim(&format_timestamp(&commit.timestamp)),
            ));
        }
        if entry.commits.is_empty() {
            lines.push(format!("    {}", paint.dim("(no commits to list)")));
        }
        lines.push(String::new());
    }

    lines
}

/// 头部一行「标签 + 值」；标签按固定宽度补齐后再着色，颜色码才不会让对齐错位。
fn label_row(paint: &Paint, label: &str, value: &str) -> String {
    let padding = " ".repeat(LABEL_WIDTH.saturating_sub(label.chars().count()));
    format!("  {}{}  {}", paint.dim(label), padding, value)
}

/// 拼出「数量 + 名词」，避免 `1 refs` 这种读起来别扭的文案。
fn count_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// 把完整 ref 名缩短成 `git branch` 里看到的名字。
///
/// `refs/heads/main` → `main`，`refs/tags/v1.0` → `v1.0`；
/// 其它形式的 ref（如 `refs/for/main`）原样返回，避免看起来像分支但实际不是。
fn short_ref(ref_name: &str) -> &str {
    ["refs/heads/", "refs/tags/"]
        .iter()
        .find_map(|prefix| ref_name.strip_prefix(*prefix))
        .filter(|name| !name.is_empty())
        .unwrap_or(ref_name)
}

/// ref 类型标签，让用户一眼看出这是分支、标签还是其它引用。
fn ref_kind(ref_name: &str) -> &'static str {
    if ref_name.starts_with("refs/heads/") {
        "branch"
    } else if ref_name.starts_with("refs/tags/") {
        "tag"
    } else {
        "ref"
    }
}

/// 把 Git 的 ISO 8601 时间截短成 `2026-10-24 09:30`（保留提交自身的时区）。
///
/// 只做字符串处理，不做时区换算；格式不认识时原样返回，不猜测。
fn format_timestamp(timestamp: &str) -> String {
    let Some((date, rest)) = timestamp.split_once('T') else {
        return timestamp.to_string();
    };
    let Some(clock) = rest.get(..5) else {
        return timestamp.to_string();
    };
    let shaped = date.len() == 10
        && clock.bytes().enumerate().all(|(index, byte)| match index {
            2 => byte == b':',
            _ => byte.is_ascii_digit(),
        });
    if shaped {
        format!("{date} {clock}")
    } else {
        timestamp.to_string()
    }
}

/// 按字符数截断过长文本，避免一条超长标题撑满终端。
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_SUBJECT_CHARS {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(MAX_SUBJECT_CHARS).collect();
    shortened.push_str("...");
    shortened
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const OID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ZERO_OID: &str = "0000000000000000000000000000000000000000";

    fn commit(subject: &str) -> CommitInfo {
        CommitInfo {
            sha: OID_A.to_string(),
            short_sha: "aaaaaaa".to_string(),
            author: "Ada".to_string(),
            timestamp: "2026-10-24T09:30:00+08:00".to_string(),
            subject: subject.to_string(),
        }
    }

    fn preview(refs: Vec<PreviewRef>) -> Preview {
        Preview {
            repository_path: "/tmp/repo".to_string(),
            remote_name: "origin".to_string(),
            remote_url: "https://example.com/team/repo.git".to_string(),
            refs,
        }
    }

    fn update(local_ref: &str, remote_oid: &str) -> RefUpdate {
        RefUpdate {
            local_ref: local_ref.to_string(),
            local_oid: OID_A.to_string(),
            remote_ref: local_ref.to_string(),
            remote_oid: remote_oid.to_string(),
        }
    }

    fn run_interaction(input: &str, lines: Vec<String>) -> (Decision, String) {
        let mut output = Vec::new();
        let decision = interact(
            Cursor::new(input.as_bytes()),
            &mut output,
            &lines,
            &Paint::plain(),
        );
        (
            decision,
            String::from_utf8(output).expect("terminal output should be UTF-8"),
        )
    }

    #[test]
    fn yes_confirms_and_no_declines() {
        let lines = vec!["summary".to_string()];
        assert_eq!(
            run_interaction("yes\n", lines.clone()).0,
            Decision::Confirmed
        );
        assert_eq!(run_interaction("Y\n", lines.clone()).0, Decision::Confirmed);
        assert_eq!(run_interaction("no\n", lines).0, Decision::Declined);
    }

    #[test]
    fn unrecognized_input_never_confirms() {
        // 空输入、无关文字都必须重新询问，不能因为「没看懂」就放行。
        let lines = vec!["summary".to_string()];
        let (decision, _) = run_interaction("\nwhatever\n", lines);
        // 输入耗尽触发 EOF，最终按取消处理。
        assert_eq!(decision, Decision::Declined);
    }

    #[test]
    fn eof_is_a_cancellation() {
        let (decision, _) = run_interaction("", vec!["summary".to_string()]);
        assert_eq!(decision, Decision::Declined);
    }

    #[test]
    fn single_page_output_has_no_page_marker() {
        // 只有一页时不显示 `[page 1/1]`，减少无意义的前缀。
        let (_, output) = run_interaction("n\n", vec!["summary".to_string()]);
        assert!(!output.contains("[page"));
        assert!(output.contains("[y] confirm"));
        assert!(output.contains("[n] cancel"));
    }

    #[test]
    fn pagination_moves_between_pages_and_keeps_the_prompt() {
        let lines: Vec<String> = (0..PAGE_LINES * 2 + 1)
            .map(|i| format!("line {i}"))
            .collect();
        let (decision, output) = run_interaction("m\nm\ny\n", lines);

        assert_eq!(decision, Decision::Confirmed);
        assert!(output.contains("[page 1/3]"));
        assert!(output.contains("[page 2/3]"));
        assert!(output.contains("[page 3/3]"));
        assert!(output.contains("[b] previous"));
    }

    #[test]
    fn render_lists_refs_commits_and_summary() {
        let lines = render(
            &preview(vec![PreviewRef {
                update: update("refs/heads/main", "b".repeat(40).as_str()),
                commits: vec![commit("feat: add confirmation")],
                rewrites_history: false,
            }]),
            &Paint::plain(),
        );
        let text = lines.join("\n");

        assert!(text.contains("push confirmation"));
        assert!(text.contains("/tmp/repo"));
        // 分支名用 `git branch` 里的简称，不再重复显示 `refs/heads/`。
        assert!(text.contains("branch: main (update)"));
        assert!(!text.contains("refs/heads/"));
        assert!(text.contains("changes"));
        assert!(text.contains("1 ref, 1 commit"));
        assert!(text.contains("aaaaaaa  feat: add confirmation"));
        // 作者与提交时间必须与 commit 在同一块里展示。
        assert!(text.contains("Ada · 2026-10-24 09:30"));
    }

    #[test]
    fn render_marks_creations_and_deletions_without_showing_placeholders() {
        let lines = render(
            &preview(vec![
                PreviewRef {
                    update: update("refs/heads/new", ZERO_OID),
                    commits: Vec::new(),
                    rewrites_history: false,
                },
                PreviewRef {
                    update: RefUpdate {
                        local_ref: "(delete)".to_string(),
                        local_oid: ZERO_OID.to_string(),
                        remote_ref: "refs/heads/old".to_string(),
                        remote_oid: "b".repeat(40),
                    },
                    commits: Vec::new(),
                    rewrites_history: false,
                },
            ]),
            &Paint::plain(),
        );
        let text = lines.join("\n");

        assert!(text.contains("branch: new (create)"));
        assert!(text.contains("branch: old (delete)"));
        // 删除时 Git 传入的 `(delete)` 占位符不能被当成分支名展示。
        assert!(!text.contains("(delete) ->"));
        assert!(text.contains("(no commits to list)"));
    }

    #[test]
    fn render_flags_history_rewrites_separately_from_plain_updates() {
        // 强制更新（非快进）会丢弃远端提交，必须与普通 update 区分。
        let lines = render(
            &preview(vec![PreviewRef {
                update: update("refs/heads/main", "b".repeat(40).as_str()),
                commits: Vec::new(),
                rewrites_history: true,
            }]),
            &Paint::plain(),
        );

        assert!(lines.join("\n").contains("branch: main (rewrite)"));
    }

    #[test]
    fn branch_names_use_the_same_short_form_as_git_branch() {
        assert_eq!(short_ref("refs/heads/feature/login"), "feature/login");
        assert_eq!(short_ref("refs/tags/v1.2.3"), "v1.2.3");
        // 非分支/标签的 ref 保持原样，避免被误认为分支。
        assert_eq!(short_ref("refs/for/main"), "refs/for/main");
        assert_eq!(ref_kind("refs/heads/main"), "branch");
        assert_eq!(ref_kind("refs/tags/v1.2.3"), "tag");
        assert_eq!(ref_kind("refs/for/main"), "ref");
    }

    #[test]
    fn renamed_refs_show_the_local_and_remote_names() {
        // `git push origin test:team-test` 这类推送两边名字不同，必须都展示。
        let lines = render(
            &preview(vec![PreviewRef {
                update: RefUpdate {
                    local_ref: "refs/heads/test".to_string(),
                    local_oid: OID_A.to_string(),
                    remote_ref: "refs/heads/team-test".to_string(),
                    remote_oid: "b".repeat(40),
                },
                commits: Vec::new(),
                rewrites_history: false,
            }]),
            &Paint::plain(),
        );

        assert!(lines
            .join("\n")
            .contains("branch: test -> team-test (update)"));
    }

    #[test]
    fn commit_times_are_shortened_to_minutes() {
        assert_eq!(
            format_timestamp("2026-10-24T09:30:00+08:00"),
            "2026-10-24 09:30"
        );
        assert_eq!(format_timestamp("2026-10-24T09:30:00Z"), "2026-10-24 09:30");
        // 不认识的格式原样返回，不凭空猜测。
        assert_eq!(
            format_timestamp("2026-10-24 09:30:00 +0800"),
            "2026-10-24 09:30:00 +0800"
        );
        assert_eq!(format_timestamp("not-a-time"), "not-a-time");
    }

    #[test]
    fn colors_come_only_from_gitguard() {
        let painted = Paint { enabled: true };
        let lines = render(
            &preview(vec![PreviewRef {
                update: update("refs/heads/main", ZERO_OID),
                commits: vec![commit("\u{1b}[31mred \u{1b}[2Jclear")],
                rewrites_history: false,
            }]),
            &painted,
        );
        let text = lines.join("\n");

        // GitGuard 自己的颜色码存在……
        assert!(text.contains("\u{1b}[1;36m"));
        // ……但仓库内容里的转义序列已被过滤，拼不出可执行的控制序列。
        assert!(!text.contains("\u{1b}[31mred"));
        assert!(!text.contains("\u{1b}[2J"));
    }

    #[test]
    fn render_strips_terminal_escape_sequences() {
        // 提交标题里带 ESC 序列时，终端不能把它当作控制指令执行。
        let lines = render(
            &preview(vec![PreviewRef {
                update: update("refs/heads/main", ZERO_OID),
                commits: vec![commit("\u{1b}[31mred\u{1b}[0m \u{202e}reversed")],
                rewrites_history: false,
            }]),
            &Paint::plain(),
        );
        let text = lines.join("\n");

        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{202e}'));
        // ESC 被替换成空格，剩下的 `[31m` 只是普通字符，不会被终端解释。
        assert!(text.contains("[31mred"));
    }

    #[test]
    fn long_subjects_are_truncated() {
        let lines = render(
            &preview(vec![PreviewRef {
                update: update("refs/heads/main", ZERO_OID),
                commits: vec![commit(&"a".repeat(MAX_SUBJECT_CHARS + 10))],
                rewrites_history: false,
            }]),
            &Paint::plain(),
        );
        let subject_line = lines
            .iter()
            .find(|line| line.trim_start().starts_with("aaaaaaa"))
            .expect("commit line should be rendered");

        assert!(subject_line.ends_with("..."));
        assert_eq!(
            subject_line.chars().count(),
            4 + 7 + 2 + MAX_SUBJECT_CHARS + 3
        );
    }

    #[test]
    fn urls_hide_embedded_credentials() {
        assert_eq!(
            redact_url("https://user:token@example.com/team/repo.git"),
            "https://***@example.com/team/repo.git"
        );
        assert_eq!(
            redact_url("https://example.com/team/repo.git"),
            "https://example.com/team/repo.git"
        );
        // scp 形式的地址没有 userinfo，保持原样（其中的 git 只是用户名，不是密码）。
        assert_eq!(
            redact_url("git@example.com:team/repo.git"),
            "git@example.com:team/repo.git"
        );
        // 路径里的 `@` 不是凭据，不能被误当成用户名。
        assert_eq!(
            redact_url("https://example.com/team/repo@v1.git"),
            "https://example.com/team/repo@v1.git"
        );
    }
}
