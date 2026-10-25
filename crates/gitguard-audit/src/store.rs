//! 审计文件的追加写、读取与保留清理。
//!
//! 文件是 JSONL：一行一个事件，追加写、不重排历史行。读取时坏行只计数跳过，
//! 不影响其它事件；保留策略在每次写入后执行，按「30 天 / 2000 条」裁剪。
//!
//! 安全约定：
//! - 文件权限在 Unix 上固定为 `0600`；
//! - 写入失败向上返回错误，由调用方只写 `stderr`，绝不改变放行判断；
//! - 清理使用「同目录临时文件 + rename」，避免留下半截文件；
//! - 读取大文件时只取尾部，并丢弃截断出来的半行。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use gitguard_core::{GitGuardError, Result};

use crate::event::AuditEvent;

/// 审计文件名，与 `config.json` 放在同一目录。
pub const AUDIT_FILE_NAME: &str = "audit.jsonl";

/// 保留天数；超期事件在清理时删除。
pub const RETENTION_DAYS: u64 = 30;

/// 保留条数上限；超过时只留最新的记录。
pub const MAX_RETENTION_ENTRIES: usize = 2000;

/// 单次读取上限：防止被篡改的超大文件拖垮界面。
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// 清理时最多读取的字节数；超出部分只保留尾部。
pub const MAX_COMPACT_BYTES: u64 = 32 * 1024 * 1024;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// 返回审计文件路径（`<配置目录>/audit.jsonl`）。
pub fn audit_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(AUDIT_FILE_NAME)
}

/// 追加一条事件并立即刷盘。
///
/// 目录不存在时自动创建；Unix 上文件权限固定为 `0600`。
pub fn append_event_to(path: &Path, event: &AuditEvent) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        GitGuardError::Internal(format!(
            "audit path {} has no parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        GitGuardError::Io(format!(
            "could not create the audit directory {}: {error}",
            parent.display()
        ))
    })?;

    let mut line = serde_json::to_string(event).map_err(|error| {
        GitGuardError::Internal(format!("could not serialize an audit event: {error}"))
    })?;
    line.push('\n');

    let mut file = open_private_append(path)?;
    file.write_all(line.as_bytes()).map_err(|error| {
        GitGuardError::Io(format!(
            "could not write the audit log {}: {error}",
            path.display()
        ))
    })?;
    // 不调用 sync_all：审计不是安全边界，避免每次推送都等待磁盘。
    file.flush().map_err(|error| {
        GitGuardError::Io(format!(
            "could not flush the audit log {}: {error}",
            path.display()
        ))
    })
}

/// 打开（必要时创建）权限为 `0600` 的追加文件。
fn open_private_append(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|error| {
        GitGuardError::Io(format!(
            "could not open the audit log {}: {error}",
            path.display()
        ))
    })?;
    restrict_permissions(path)?;
    Ok(file)
}

/// 读取审计文件的尾部。
#[derive(Debug, Default)]
pub struct AuditTail {
    /// 事件按「旧 → 新」排列，最多 `limit` 条。
    pub events: Vec<AuditEvent>,
    /// 无法解析的完整行数量（截断出来的半行不计入）。
    pub skipped_lines: usize,
    /// 文件超过读取上限，只读取了尾部。
    pub truncated: bool,
}

/// 读取审计文件尾部最多 `limit` 条事件。
///
/// - 文件不存在时返回空结果（首次运行属正常情况）；
/// - 超过 [`MAX_READ_BYTES`] 时只读尾部，并在结果里标记 `truncated`；
/// - 坏行只计数，不返回任何猜测出来的事件。
pub fn read_events_from(path: &Path, limit: usize) -> Result<AuditTail> {
    let (bytes, truncated) = match read_bounded(path, MAX_READ_BYTES) {
        Ok(Some(result)) => result,
        Ok(None) => return Ok(AuditTail::default()),
        Err(error) => return Err(error),
    };
    Ok(parse_tail(&bytes, limit, truncated))
}

/// 按保留策略裁剪审计文件；没有需要删除的内容时不重写文件。
///
/// 清理失败只应记录错误，不应影响推送结果。
pub fn enforce_retention_at(path: &Path, now_epoch_secs: u64) -> Result<()> {
    let (bytes, truncated) = match read_bounded(path, MAX_COMPACT_BYTES) {
        Ok(Some(result)) => result,
        Ok(None) => return Ok(()),
        Err(error) => return Err(error),
    };

    let cutoff = now_epoch_secs.saturating_sub(RETENTION_DAYS * SECONDS_PER_DAY);
    let mut kept: Vec<&[u8]> = Vec::new();
    let mut removed_any = truncated;

    let mut lines = bytes.split(|byte| *byte == b'\n');
    if truncated {
        // 从文件中部截出来的第一段不是完整行，直接丢弃。
        lines.next();
    }
    for line in lines {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Some(event) if event.timestamp_epoch_secs >= cutoff => kept.push(line),
            _ => removed_any = true,
        }
    }

    if kept.len() > MAX_RETENTION_ENTRIES {
        let excess = kept.len() - MAX_RETENTION_ENTRIES;
        kept.drain(0..excess);
        removed_any = true;
    }

    if !removed_any {
        return Ok(());
    }

    let mut content = Vec::new();
    for line in kept {
        content.extend_from_slice(line);
        content.push(b'\n');
    }
    write_atomic_private(path, &content)
}

/// 解析尾部内容；`truncated` 为真时丢弃第一段半行。
fn parse_tail(bytes: &[u8], limit: usize, truncated: bool) -> AuditTail {
    let mut tail = AuditTail {
        truncated,
        ..AuditTail::default()
    };
    let mut events = Vec::new();

    let mut lines = bytes.split(|byte| *byte == b'\n');
    if truncated {
        lines.next();
    }
    for line in lines {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Some(event) => events.push(event),
            None => tail.skipped_lines += 1,
        }
    }

    if events.len() > limit {
        events.drain(0..events.len() - limit);
    }
    tail.events = events;
    tail
}

/// 解析一行 JSONL；非法 UTF-8 或结构不完整都返回 `None`。
fn parse_line(line: &[u8]) -> Option<AuditEvent> {
    let text = std::str::from_utf8(line).ok()?;
    serde_json::from_str(text).ok()
}

/// 读取文件（最多 `max_bytes` 字节）。
///
/// 返回 `None` 表示文件不存在；第二个元组项表示是否从中间截断。
fn read_bounded(path: &Path, max_bytes: u64) -> Result<Option<(Vec<u8>, bool)>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(GitGuardError::Io(format!(
                "could not open the audit log {}: {error}",
                path.display()
            )));
        }
    };

    let length = file
        .metadata()
        .map_err(|error| {
            GitGuardError::Io(format!(
                "could not stat the audit log {}: {error}",
                path.display()
            ))
        })?
        .len();
    if length == 0 {
        return Ok(Some((Vec::new(), false)));
    }

    let truncated = length > max_bytes;
    if truncated {
        file.seek(SeekFrom::Start(length - max_bytes))
            .map_err(|error| {
                GitGuardError::Io(format!(
                    "could not seek in the audit log {}: {error}",
                    path.display()
                ))
            })?;
    }

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| {
        GitGuardError::Io(format!(
            "could not read the audit log {}: {error}",
            path.display()
        ))
    })?;
    Ok(Some((bytes, truncated)))
}

/// 原子替换文件：同目录临时文件写完并刷盘后再 rename。
fn write_atomic_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        GitGuardError::Internal(format!(
            "audit path {} has no parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        GitGuardError::Io(format!(
            "could not create the audit directory {}: {error}",
            parent.display()
        ))
    })?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(AUDIT_FILE_NAME);
    let temp_path = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));

    if let Err(error) = write_private(&temp_path, bytes, false) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(GitGuardError::Io(format!(
            "could not replace the audit log {}: {error}",
            path.display()
        )));
    }
    Ok(())
}

/// 新建一个权限为 `0600` 的文件；目标已存在时失败，不覆盖用户文件。
pub(crate) fn write_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            GitGuardError::Io(format!(
                "could not create the directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    write_private(path, bytes, true)
}

/// 写出完整内容并刷盘；`create_new` 为真时拒绝覆盖已有文件。
fn write_private(path: &Path, bytes: &[u8], create_new: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path).map_err(|error| {
        GitGuardError::Io(format!("could not create {}: {error}", path.display()))
    })?;
    restrict_permissions(path)?;
    file.write_all(bytes).map_err(|error| {
        GitGuardError::Io(format!("could not write {}: {error}", path.display()))
    })?;
    file.sync_all()
        .map_err(|error| GitGuardError::Io(format!("could not flush {}: {error}", path.display())))
}

/// 把文件权限收紧到 `0600`（仅 Unix）；其它平台由系统默认权限决定。
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        GitGuardError::Io(format!(
            "could not restrict the permissions of {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{reason, AuditChannel, AuditEventKind, AuditResult, AuditSubject};

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary directory should be created")
    }

    fn event_at(timestamp_epoch_secs: u64, kind: AuditEventKind) -> AuditEvent {
        AuditSubject::new("/tmp/repo", None, "origin", "git@example.com:a/b.git", 1).event(
            timestamp_epoch_secs,
            kind,
            AuditResult::Pending,
            reason::CONFIRMATION_REQUIRED,
            None,
            AuditChannel::Desktop,
        )
    }

    #[test]
    fn appending_then_reading_round_trips_events() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        assert!(!path.exists(), "测试前提：文件不存在");

        let first = event_at(1_700_000_000, AuditEventKind::RequestCreated);
        let second = event_at(1_700_000_001, AuditEventKind::Confirmed);
        append_event_to(&path, &first).expect("append first");
        append_event_to(&path, &second).expect("append second");

        let tail = read_events_from(&path, 10).expect("read audit log");
        assert_eq!(tail.events, vec![first, second]);
        assert_eq!(tail.skipped_lines, 0);
        assert!(!tail.truncated);
    }

    #[test]
    fn missing_file_reads_as_an_empty_tail() {
        let dir = tempdir();
        let tail =
            read_events_from(&audit_file_path(dir.path()), 10).expect("missing file is fine");
        assert!(tail.events.is_empty());
        assert!(!tail.truncated);
    }

    #[test]
    fn limit_keeps_only_the_newest_events() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        for index in 0..5 {
            append_event_to(
                &path,
                &event_at(1_700_000_000 + index, AuditEventKind::Failure),
            )
            .expect("append");
        }

        let tail = read_events_from(&path, 2).expect("read tail");
        assert_eq!(tail.events.len(), 2);
        assert_eq!(tail.events[0].timestamp_epoch_secs, 1_700_000_003);
        assert_eq!(tail.events[1].timestamp_epoch_secs, 1_700_000_004);
    }

    #[test]
    fn malformed_lines_are_counted_and_skipped() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        append_event_to(
            &path,
            &event_at(1_700_000_000, AuditEventKind::RequestCreated),
        )
        .expect("append");

        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open audit log");
        file.write_all(b"{ not json }\n\n")
            .expect("write malformed line");

        let tail = read_events_from(&path, 10).expect("read audit log");
        assert_eq!(tail.events.len(), 1);
        assert_eq!(tail.skipped_lines, 1);
    }

    #[test]
    fn retention_drops_expired_and_malformed_lines() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        let now = 1_700_000_000;
        let expired = event_at(
            now - RETENTION_DAYS * SECONDS_PER_DAY - 1,
            AuditEventKind::Timeout,
        );
        let fresh = event_at(now, AuditEventKind::Confirmed);
        append_event_to(&path, &expired).expect("append expired");
        append_event_to(&path, &fresh).expect("append fresh");
        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open audit log");
            file.write_all(b"broken line\n").expect("write broken line");
        }

        enforce_retention_at(&path, now).expect("apply retention");

        let tail = read_events_from(&path, 10).expect("read after cleanup");
        assert_eq!(tail.events, vec![fresh]);
        assert_eq!(tail.skipped_lines, 0, "损坏行应已被清理");
    }

    #[test]
    fn retention_keeps_the_newest_entries_above_the_limit() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        let now = 1_700_000_000;
        for index in 0..(MAX_RETENTION_ENTRIES + 5) {
            append_event_to(
                &path,
                &event_at(now + index as u64, AuditEventKind::Failure),
            )
            .expect("append");
        }

        enforce_retention_at(&path, now + MAX_RETENTION_ENTRIES as u64).expect("apply retention");

        let tail = read_events_from(&path, MAX_RETENTION_ENTRIES + 10).expect("read after cleanup");
        assert_eq!(tail.events.len(), MAX_RETENTION_ENTRIES);
        // 保留的是最后写入的 2000 条。
        assert_eq!(
            tail.events.last().expect("last event").timestamp_epoch_secs,
            now + (MAX_RETENTION_ENTRIES + 4) as u64
        );
    }

    #[test]
    fn retention_does_not_rewrite_an_unchanged_file() {
        let dir = tempdir();
        let path = audit_file_path(dir.path());
        append_event_to(&path, &event_at(1_700_000_000, AuditEventKind::Confirmed))
            .expect("append");
        let before = fs::metadata(&path)
            .expect("stat audit log")
            .modified()
            .expect("mtime");

        enforce_retention_at(&path, 1_700_000_100).expect("apply retention");

        let after = fs::metadata(&path)
            .expect("stat audit log")
            .modified()
            .expect("mtime");
        assert_eq!(before, after, "没有需要清理的内容时不应重写文件");
    }

    #[cfg(unix)]
    #[test]
    fn audit_file_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir();
        let path = audit_file_path(dir.path());
        append_event_to(&path, &event_at(1_700_000_000, AuditEventKind::Confirmed))
            .expect("append");

        let mode = fs::metadata(&path)
            .expect("stat audit log")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn oversized_files_are_read_from_the_tail_and_marked_truncated() {
        // 直接用私有函数验证截断语义，避免测试真的写 8 MiB。
        let line = serde_json::to_vec(&event_at(1_700_000_000, AuditEventKind::Failure))
            .expect("serialize");
        let mut bytes = b"{\"partial\":".to_vec();
        bytes.push(b'\n');
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');

        let tail = parse_tail(&bytes, 10, true);
        assert!(tail.truncated);
        assert_eq!(tail.events.len(), 1);
        assert_eq!(tail.skipped_lines, 0, "截断出来的半行不应算作损坏行");
    }

    #[test]
    fn writing_a_new_private_file_refuses_to_overwrite() {
        let dir = tempdir();
        let path = dir.path().join("diagnostics.txt");
        write_new_private(&path, b"first").expect("create export");
        assert!(
            write_new_private(&path, b"second").is_err(),
            "不得覆盖已有文件"
        );
    }
}
