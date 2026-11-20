use crate::{
    protocol,
    svn::{self, Entry, Info, LogEntry},
    workspace::Scope,
};
use openidea_plugin::{ContentDiff, Error, FileDecoration, FileKind, host};

pub fn error(message: impl Into<String>) -> Error {
    Error::Failed(message.into())
}

pub fn svn_error(stderr: &str) -> Error {
    let detail: String = stderr.chars().take(5000).collect();
    let guidance = if [
        "E170001",
        "E215004",
        "E170013",
        "Authentication failed",
        "authorization failed",
    ]
    .iter()
    .any(|s| stderr.contains(s))
    {
        "连接或认证失败。请点击「认证」在 IDE 终端完成登录，然后重试；同时检查网络及仓库权限。\n"
    } else if stderr.contains("E155004") || stderr.contains("E155037") {
        "工作副本被锁定，请确认没有其他 SVN 操作后点击「清理」。\n"
    } else if stderr.contains("E155007") {
        "当前项目不是 SVN 工作副本，请打开已检出的项目目录。\n"
    } else {
        "SVN 操作失败：\n"
    };
    error(format!("{guidance}{detail}"))
}

/// One-shot reads use the host's bounded output API: structured XML must never
/// be decoded from the rolling buffer of a streaming process handle.
pub fn read(project: &Scope, args: &[String]) -> Result<String, Error> {
    read_result(|| {
        project
            .project
            .run_process("svn", &project.args(args), None)
    })
}

fn read_result(
    run: impl FnOnce() -> Result<openidea_plugin::ProcessOutput, Error>,
) -> Result<String, Error> {
    let task = host::begin_task("SVN：读取工作副本", true);
    task.report(0, None, Some("正在读取 SVN 信息…"));
    let output = run()?;
    if task.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if output.status != Some(0) {
        return Err(svn_error(&output.stderr));
    }
    if output.stdout.len() > 8 * 1024 * 1024 {
        return Err(error("SVN 信息超过 8 MiB，请缩小项目范围。"));
    }
    Ok(output.stdout)
}

pub fn refresh(project: &Scope) -> Result<(Info, Vec<Entry>), Error> {
    refresh_with_ignored(project, false)
}
pub fn refresh_with_ignored(
    project: &Scope,
    include_ignored: bool,
) -> Result<(Info, Vec<Entry>), Error> {
    let info = svn::parse_info(&read(project, &svn::command("info", &[], "", None)?)?)?;
    let mut status = svn::command("status", &[], "", None)?;
    if include_ignored {
        status.insert(1, "--no-ignore".into());
    }
    let mut entries = svn::parse_status_at(&read(project, &status)?, Some(&project.root))?;
    let mut metadata = Vec::new();
    let mut shallow = Vec::new();
    for entry in &mut entries {
        if entry.status == "external" {
            continue;
        }
        if let Ok(stat) = project.project.stat(&project.absolute(&entry.path)) {
            entry.kind = match stat.kind {
                FileKind::Directory => "dir",
                FileKind::Symlink => "symlink",
                _ => "file",
            }
            .into();
            if entry.kind == "dir"
                && entry.path != "."
                && project
                    .project
                    .exists(&project.absolute(&format!("{}/.svn", entry.path)))
                    .unwrap_or(false)
            {
                entry.status = "external".into();
            }
        }
        if matches!(entry.status.as_str(), "deleted" | "replaced" | "missing") {
            metadata.push(entry.path.clone());
        } else if !project.direct && !matches!(entry.status.as_str(), "unversioned" | "ignored") {
            shallow.push(entry.path.clone());
        }
    }
    // --depth infinity reveals children hidden by `svn status` under a scheduled deletion.
    for paths in metadata.chunks(100) {
        let xml = read(project, &svn::command("info-tree", paths, "", None)?)?;
        svn::enrich_info_at(&mut entries, &xml, Some(&project.root))?;
    }
    for paths in shallow.chunks(100) {
        let xml = read(project, &svn::command("info", paths, "", None)?)?;
        svn::enrich_info_at(&mut entries, &xml, Some(&project.root))?;
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    decorate(project, &entries);
    Ok((info, entries))
}

fn decorate(project: &Scope, entries: &[Entry]) {
    let Ok(root) = project.root() else { return };
    let set = format!("svn-{root}");
    let _ = host::clear_file_decorations(&set);
    for entry in entries.iter().take(10000) {
        let (badge, style) = if entry.tree_conflicted {
            ("C", "error")
        } else {
            match entry.status.as_str() {
                "added" => ("A", "added"),
                "deleted" | "missing" => ("D", "removed"),
                "conflicted" => ("C", "error"),
                "unversioned" => ("?", "muted"),
                "ignored" => ("I", "muted"),
                "external" => ("X", "muted"),
                "normal" => ("P", "modified"),
                _ => ("M", "modified"),
            }
        };
        let path = if entry.path == "." {
            root.clone()
        } else {
            format!("{}/{}", root.trim_end_matches('/'), entry.path)
        };
        let _ = host::set_file_decoration(
            &path,
            &set,
            &FileDecoration {
                badge: Some(badge.into()),
                style: style.into(),
                tooltip: Some(format!("SVN: {}", entry.status)),
            },
        );
    }
}

pub fn dirty(project: &Scope, paths: &[String], all: bool) -> Result<(), Error> {
    let root = project.root()?;
    let dirty: Vec<_> = host::open_editors()?
        .into_iter()
        .filter(|e| e.dirty)
        .filter_map(|e| crate::workspace::relative_path(&root, &e.path))
        .collect();
    protocol::check_dirty_case(&dirty, paths, all, crate::workspace::windows_path(&root))
        .map_err(error)
}

pub fn local_diff(project: &Scope, path: &str, entries: &[Entry]) -> Result<(), Error> {
    svn::target(path)?;
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .ok_or("文件状态已变化，请刷新。")?;
    if entry.kind == "dir" {
        return Err(error(
            "目录没有文本内容。目录属性变化可在 SVN 输出或终端中查看。",
        ));
    }
    if entry.status == "external" {
        return Err(error("请将外部工作副本单独作为项目打开。"));
    }
    let left = if matches!(entry.status.as_str(), "added" | "unversioned" | "ignored") {
        String::new()
    } else {
        svn::text_content(
            read(
                project,
                &svn::command("cat-base", &[path.into()], "", None)?,
            )?
            .as_bytes(),
        )?
    };
    // read_file includes unsaved editor text; diff describes what the user sees.
    let right = if entry.status == "deleted" || entry.status == "missing" {
        String::new()
    } else {
        let bytes = project.project.read_bytes(&project.absolute(path));
        if !project.direct && bytes.is_err() {
            dirty(project, &[path.into()], false)?;
            saved_content(project, path)?
        } else {
            let bytes = bytes?;
            svn::text_content(&bytes)?;
            let text = project.project.read_file(&project.absolute(path))?;
            svn::text_content(text.as_bytes())?
        }
    };
    host::open_diff(
        &ContentDiff {
            left,
            right,
            left_label: Some(
                if entry.status == "added" {
                    "新增文件"
                } else {
                    "BASE"
                }
                .into(),
            ),
            right_label: Some(
                if project.direct {
                    "本地（含未保存修改）"
                } else {
                    "本地（已保存）"
                }
                .into(),
            ),
            title: Some(format!("SVN · {path}")),
        },
        path.rsplit('/').next(),
    )
}

pub fn history_diff(
    project: &Scope,
    info: &Info,
    logs: &[LogEntry],
    revision: u64,
    path: &str,
) -> Result<(), Error> {
    let log = logs
        .iter()
        .find(|l| l.revision == revision)
        .ok_or("请从已加载的日志选择版本。")?;
    let change = log
        .paths
        .iter()
        .find(|p| p.path == path)
        .ok_or("该路径不在所选版本的改动列表中。")?;
    if change.kind == "dir" {
        return Err(error("所选路径为目录，没有文本差异。"));
    }
    fn cat(project: &Scope, root: &str, path: &str, rev: u64) -> Result<String, Error> {
        if !path.starts_with('/')
            || path.chars().any(char::is_control)
            || path.split('/').any(|p| p == "..")
        {
            return Err(error("无效的仓库路径。"));
        }
        let encoded: String = path
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"/._-~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let url = format!("{}{encoded}@{rev}", root.trim_end_matches('/'));
        let args = vec![
            "cat".into(),
            "--non-interactive".into(),
            "--revision".into(),
            rev.to_string(),
            "--".into(),
            url,
        ];
        svn::text_content(read(project, &args)?.as_bytes()).map_err(error)
    }
    let left = if change.action == "A" || revision == 0 {
        String::new()
    } else {
        cat(project, &info.root_url, path, revision - 1)?
    };
    let right = if change.action == "D" {
        String::new()
    } else {
        cat(project, &info.root_url, path, revision)?
    };
    host::open_diff(
        &ContentDiff {
            left,
            right,
            left_label: Some(format!("r{}", revision.saturating_sub(1))),
            right_label: Some(format!("r{revision}")),
            title: Some(format!("SVN r{revision} · {path}")),
        },
        path.rsplit('/').next(),
    )
}

fn run_with_diff_locale(
    args: &[String],
    mut spawn: impl FnMut(&host::Command) -> Result<openidea_plugin::ProcessOutput, Error>,
) -> Result<openidea_plugin::ProcessOutput, Error> {
    // GUI processes can inherit Chinese LC_MESSAGES with an ASCII LC_CTYPE.
    // The localized diff labels then fail before SVN emits any content. Keep
    // only this machine-read diff in a UTF-8, untranslated process environment.
    let mut command = host::Command::new("svn")
        .args(args)
        .env("LC_ALL", "C.UTF-8")
        .env("LANGUAGE", "C");
    loop {
        match spawn(&command) {
            Err(Error::Invalid(message)) => {
                // OpenIDEA only permits adding variables. This error is raised
                // BEFORE spawn, so dropping the colliding addition is safe.
                let collision = command.env.iter().position(|(key, _)| {
                    let prefix = format!("environment variable {key:?} already exists");
                    message == prefix
                        || message == format!("{prefix}; only new variables may be added")
                });
                let Some(index) = collision else {
                    return Err(Error::Invalid(message));
                };
                command.env.remove(index);
            }
            result => return result,
        }
    }
}

fn saved_content(project: &Scope, path: &str) -> Result<String, Error> {
    let empty = format!(
        "{}/svn-empty",
        host::fs::plugin_dir()?.trim_end_matches('/')
    );
    host::fs::write_file(&empty, "")?;
    // SVN's internal diff can read an attached file without exposing a shell or
    // an additional executable. Validate the reconstructed bytes as text below;
    // --force avoids locale-dependent SVN classification of UTF-8 as binary.
    let args = vec![
        "diff".into(),
        "--internal-diff".into(),
        "--force".into(),
        "--ignore-properties".into(),
        "--non-interactive".into(),
        "--old".into(),
        format!("{empty}@"),
        "--new".into(),
        format!("{}@", project.absolute(path)),
    ];
    let patch = read_result(|| {
        run_with_diff_locale(&args, |command| {
            host::run_command(command, Some(&project.project), None)
        })
    })?;
    empty_diff_content(&patch).map_err(error)
}

fn empty_diff_content(patch: &str) -> Result<String, String> {
    if patch.is_empty() {
        return Ok(String::new());
    }
    let mut lines = patch.split_inclusive('\n');
    let header = lines
        .find(|line| line.starts_with("@@ "))
        .ok_or("此路径不是可显示的 UTF-8 文本文件（可能是目录或二进制文件）。")?;
    let count = header
        .strip_prefix("@@ -0,0 +1,")
        .and_then(|s| s.split_once(" @@"))
        .and_then(|(n, _)| n.parse::<usize>().ok())
        .or_else(|| header.starts_with("@@ -0,0 +1 @@").then_some(1))
        .ok_or("无法完整读取本地文件差异。")?;
    let mut text = String::new();
    let mut seen = 0;
    let mut no_newline = false;
    for line in lines {
        if let Some(content) = line.strip_prefix('+') {
            if no_newline || seen >= count {
                return Err("本地文件差异不完整。".into());
            }
            text.push_str(content);
            seen += 1;
        } else if line.trim_end_matches(['\r', '\n']) == "\\ No newline at end of file"
            && seen == count
            && !no_newline
        {
            if !text.ends_with('\n') {
                return Err("本地文件差异不完整。".into());
            }
            text.pop();
            no_newline = true;
        } else {
            return Err("无法将该路径作为单个文本文件显示。".into());
        }
    }
    if seen != count {
        return Err("本地文件差异不完整。".into());
    }
    if text.contains('\u{fffd}') {
        return Err("文件编码不能可靠地显示，请在编辑器中打开检查。".into());
    }
    svn::text_content(text.as_bytes())
}

#[cfg(test)]
mod attached_tests {
    use super::*;
    #[test]
    fn reconstructs_only_complete_empty_to_text_diffs() {
        assert_eq!(empty_diff_content("Index: a\n--- a\n+++ a\n@@ -0,0 +1,2 @@\n+中文\r\n++++last\n\\ No newline at end of file\n").unwrap(), "中文\r\n+++last");
        assert_eq!(empty_diff_content("").unwrap(), "");
        assert!(empty_diff_content("Cannot display: file marked as a binary type.").is_err());
        assert!(empty_diff_content("@@ -0,0 +1,2 @@\n+truncated\n").is_err());
        assert!(empty_diff_content("@@ -1 +1 @@\n-old\n+new\n").is_err());
    }
    #[test]
    fn actual_svn_empty_diff_preserves_utf8_crlf_and_missing_final_newline() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty");
        let file = temp.path().join("中文@.txt");
        std::fs::write(&empty, "").unwrap();
        for text in ["", "中文\n", "line\r\n+++last", "\n\n"] {
            std::fs::write(&file, text).unwrap();
            let result = std::process::Command::new("svn")
                .args([
                    "diff",
                    "--internal-diff",
                    "--force",
                    "--ignore-properties",
                    "--old",
                    &format!("{}@", empty.display()),
                    "--new",
                    &format!("{}@", file.display()),
                ])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                empty_diff_content(&String::from_utf8(result.stdout).unwrap()).unwrap(),
                text
            );
        }
        std::fs::write(&file, b"binary\0data").unwrap();
        let result = std::process::Command::new("svn")
            .args([
                "diff",
                "--internal-diff",
                "--force",
                "--ignore-properties",
                "--old",
                &format!("{}@", empty.display()),
                "--new",
                &format!("{}@", file.display()),
            ])
            .output()
            .unwrap();
        assert!(empty_diff_content(&String::from_utf8_lossy(&result.stdout)).is_err());
    }

    #[test]
    fn actual_property_bearing_file_diff_ignores_properties() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        assert!(
            std::process::Command::new("svnadmin")
                .arg("create")
                .arg(&repo)
                .status()
                .unwrap()
                .success()
        );
        let wc = temp.path().join("wc");
        let run = |args: &[&str]| {
            let output = std::process::Command::new("svn")
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };
        run(&[
            "co",
            &format!("file://{}", repo.display()),
            wc.to_str().unwrap(),
        ]);
        let path = wc.join("a.txt");
        std::fs::write(&path, "base\n").unwrap();
        run(&["add", path.to_str().unwrap()]);
        run(&["propset", "svn:eol-style", "native", path.to_str().unwrap()]);
        run(&["propset", "svn:keywords", "Id", path.to_str().unwrap()]);
        run(&["ci", wc.to_str().unwrap(), "-m", "seed"]);
        std::fs::write(&path, "modified 中文\n").unwrap();
        let empty = temp.path().join("empty");
        std::fs::write(&empty, "").unwrap();
        let patch = run(&[
            "diff",
            "--internal-diff",
            "--force",
            "--ignore-properties",
            "--old",
            empty.to_str().unwrap(),
            "--new",
            path.to_str().unwrap(),
        ]);
        assert_eq!(
            empty_diff_content(&String::from_utf8(patch.stdout).unwrap()).unwrap(),
            "modified 中文\n"
        );
    }
    #[test]
    fn attached_diff_works_with_chinese_messages_and_ascii_gui_encoding() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("svn-empty");
        let path = temp.path().join("file.js");
        std::fs::write(&empty, "").unwrap();
        std::fs::write(&path, "const value = \"中文\";").unwrap();
        let args: Vec<String> = [
            "diff",
            "--internal-diff",
            "--force",
            "--ignore-properties",
            "--non-interactive",
            "--old",
            &format!("{}@", empty.display()),
            "--new",
            &format!("{}@", path.display()),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let output = run_with_diff_locale(&args, |cmd| {
            let mut process = std::process::Command::new("svn");
            for (key, _) in std::env::vars_os() {
                if key
                    .to_str()
                    .is_some_and(|key| key.starts_with("LC_") || matches!(key, "LANG" | "LANGUAGE"))
                {
                    process.env_remove(key);
                }
            }
            let output = process
                .args(&cmd.args)
                .env("LANG", "zh_CN.UTF-8")
                .env("LC_CTYPE", "C")
                .envs(cmd.env.iter().cloned())
                .output()
                .unwrap();
            Ok(openidea_plugin::ProcessOutput {
                status: output.status.code(),
                stdout: String::from_utf8(output.stdout).unwrap(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        })
        .unwrap();
        assert_eq!(output.status, Some(0), "{}", output.stderr);
        assert_eq!(
            empty_diff_content(&output.stdout).unwrap(),
            "const value = \"中文\";"
        );
    }

    #[test]
    fn inherited_locale_conflicts_are_removed_before_starting_diff() {
        let mut attempts = 0;
        let output = run_with_diff_locale(&["diff".into()], |cmd| {
            attempts += 1;
            if let Some((key, _)) = cmd.env.first() {
                return Err(Error::Invalid(format!(
                    "environment variable {key:?} already exists; only new variables may be added"
                )));
            }
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: "unchanged".into(),
                stderr: String::new(),
            })
        })
        .unwrap();
        assert_eq!(output.stdout, "unchanged");
        assert_eq!(attempts, 3);
    }

    #[test]
    fn completed_or_denied_diff_is_never_retried() {
        let mut attempts = 0;
        let output = run_with_diff_locale(&["diff".into()], |_| {
            attempts += 1;
            Ok(openidea_plugin::ProcessOutput {
                status: Some(1),
                stdout: String::new(),
                stderr: "svn: E000022".into(),
            })
        })
        .unwrap();
        assert_eq!(output.status, Some(1));
        assert_eq!(attempts, 1);
        let result = run_with_diff_locale(&["diff".into()], |_| {
            attempts += 1;
            Err::<openidea_plugin::ProcessOutput, _>(Error::Invalid(
                "not an environment collision".into(),
            ))
        });
        assert!(result.is_err());
        assert_eq!(attempts, 2);
    }
    #[test]
    fn saved_content_passes_utf8_environment_to_the_host() {
        use openidea_plugin::mock::MockHost;
        let host =
            MockHost::install().with_capabilities(&["project.read", "process:svn", "storage"]);
        let project = Scope::new(host.invocation().project().unwrap(), "/attached".into());
        host.on_process("svn", |_, _| {
            Ok(openidea_plugin::ProcessOutput {
                status: Some(0),
                stdout: "@@ -0,0 +1 @@\n+中文\n".into(),
                stderr: String::new(),
            })
        });
        assert_eq!(saved_content(&project, "file.js").unwrap(), "中文\n");
        let runs = host.process_runs();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].env.contains(&("LC_ALL".into(), "C.UTF-8".into())));
        assert!(runs[0].env.contains(&("LANGUAGE".into(), "C".into())));
    }
}

/// Prepare one parent-directory property change; writing uses the normal job/lock.
pub fn prepare_ignore(
    project: &Scope,
    paths: &[String],
    remove: bool,
    client: &str,
) -> Result<Option<Vec<String>>, Error> {
    use crate::ignore_rules as rules;
    if paths.len() != 1 {
        return Err(error(
            "请点击一个未跟踪或已忽略的文件/目录，再执行忽略操作。",
        ));
    }
    let path = &paths[0];
    svn::target(path)?;
    let expected = if remove { "ignored" } else { "unversioned" };
    let (_, entries) = refresh_with_ignored(project, remove)?;
    if !entries
        .iter()
        .any(|e| e.path == *path && e.status == expected)
    {
        return Err(error(if remove {
            "仅能取消已忽略项目的忽略规则，请开启「显示已忽略」并选择目标。"
        } else {
            "仅能忽略未跟踪的文件或目录；已跟踪文件请先明确处理版本控制状态。"
        }));
    }
    let (parent, name) = path.rsplit_once('/').unwrap_or((".", path.as_str()));
    let info = svn::parse_info(&read(
        project,
        &svn::command("info", &[parent.into()], "", None)?,
    )?)?;
    if info.kind != "dir" {
        return Err(error("忽略规则只能写入已跟踪的父目录。"));
    }
    let args = svn::command("ignore-list", &[parent.into()], "", None)?;
    let old = rules::parse(&read(project, &args)?)?;
    let value = if remove {
        if !rules::matches_any(old.as_deref().unwrap_or(""), name) {
            return Err(error(
                "本目录的 svn:ignore 没有匹配此项目的规则。该项可能来自 svn:global-ignores、继承或 SVN 客户端全局配置；本目录的其他规则不会改动。",
            ));
        }
        match rules::update(old.as_deref(), name, true) {
            Ok(value) if !rules::matches_any(value.as_deref().unwrap_or(""), name) => value,
            _ => {
                let Some(edited) = host::input_multiline(
                    &format!(
                        "取消忽略 {path}：请删除使该项目匹配的规则。目录 {parent} 的其他规则请保留；删除通配符会影响同目录其他项目。"
                    ),
                    old.as_deref(),
                )?
                else {
                    return Ok(None);
                };
                rules::validate(&edited)?;
                if edited == old.as_deref().unwrap_or("") {
                    return Ok(None);
                }
                if rules::matches_any(&edited, name) {
                    return Err(error(
                        "修改后的本目录规则仍匹配此项目，请删除实际匹配的规则后重试。",
                    ));
                }
                if edited.is_empty() {
                    None
                } else {
                    Some(edited)
                }
            }
        }
    } else {
        rules::update(old.as_deref(), name, false)?
    };
    let body = format!(
        "目标：{path}\n规则目录：{parent}\n\n{}\n\n修改后的 svn:ignore：\n{}",
        if remove {
            "只修改本目录的 svn:ignore，不修改或删除本地文件；继承和全局忽略规则仍有效。"
        } else {
            "文件保留在本地；忽略规则是父目录的属性变更，可随后提交。"
        },
        value
            .as_deref()
            .unwrap_or("（删除本目录的 svn:ignore 属性）")
    );
    if !host::confirm(
        if remove {
            "确认取消忽略"
        } else {
            "确认忽略所选项目"
        },
        &body,
    )? {
        return Ok(None);
    }
    if rules::parse(&read(project, &args)?)? != old {
        return Err(error(
            "忽略规则在确认期间已变化，请刷新后重试，避免覆盖其他修改。",
        ));
    }
    let (_, latest) = refresh_with_ignored(project, remove)?;
    if !latest
        .iter()
        .any(|e| e.path == *path && e.status == expected)
    {
        return Err(error("所选项目的 SVN 状态已变化，请刷新后重试。"));
    }
    if rules::parse(&read(project, &args)?)? != old {
        return Err(error(
            "忽略规则在确认期间已变化，请刷新后重试，避免覆盖其他修改。",
        ));
    }
    let file = if let Some(value) = value {
        let file = format!(
            "{}/svn-ignore-{client}.txt",
            host::fs::plugin_dir()?.trim_end_matches('/')
        );
        host::fs::write_file(&file, &value)?;
        Some(file)
    } else {
        None
    };
    Ok(Some(rules::command(parent, file.as_deref())?))
}

pub fn check_revert_directory(project: &Scope, path: &str) -> Result<(), Error> {
    svn::target(path)?;
    let (_, entries) = refresh_with_ignored(project, true)?;
    svn::validate_revert_tree(&entries, path)?;
    check_deleted_paths(
        project,
        entries
            .iter()
            .filter(|e| path == "." || e.path == path || e.path.starts_with(&format!("{path}/"))),
    )?;
    if !entries.iter().any(|e| {
        (path == "." || e.path == path || e.path.starts_with(&format!("{path}/")))
            && !matches!(e.status.as_str(), "unversioned" | "ignored" | "external")
    }) {
        return Err(error("所选目录没有可还原的已跟踪变更，请刷新。"));
    }
    if svn::parse_info(&read(
        project,
        &svn::command("info", &[path.into()], "", None)?,
    )?)?
    .kind
        != "dir"
    {
        return Err(error("目录还原必须选择已跟踪目录。"));
    }
    Ok(())
}
pub fn selected_revert_paths(
    project: &Scope,
    entries: &[Entry],
    paths: &[String],
) -> Result<Vec<String>, Error> {
    let ordered = svn::order_revert(entries, paths).map_err(error)?;
    check_deleted_paths(project, entries.iter().filter(|e| paths.contains(&e.path)))?;

    for entry in entries.iter().filter(|e| {
        paths.contains(&e.path) && e.status == "added" && e.kind != "file" && e.kind != "symlink"
    }) {
        let xml = read(
            project,
            &svn::command("info-tree", std::slice::from_ref(&entry.path), "", None)?,
        )?;
        let nodes = svn::info_paths(&xml, &project.root)?;
        if nodes.iter().any(|path| !paths.contains(path)) {
            return Err(error(
                "新增目录含未选中的后代。请选中目录行后执行整个目录还原，或只还原明确勾选的文件。",
            ));
        }
    }
    Ok(ordered)
}

#[cfg(test)]
mod ignore_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    use std::{cell::Cell, rc::Rc};
    fn setup(status: &'static str, properties: impl Fn() -> String + 'static) -> (MockHost, Scope) {
        let host = MockHost::install().with_capabilities(&[
            "project.read",
            "process:svn",
            "storage",
            "editor.read",
        ]);
        host.on_process("svn",move |args,_|{
            let stdout=match args[0].as_str(){
                "status"=>format!("<status><target path=\".\"><entry path=\"nested/cache.tmp\"><wc-status item=\"{status}\" props=\"none\"/></entry></target></status>"),
                "proplist"=>properties(),
                _=>"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><wc-info><wcroot-abspath>/second</wcroot-abspath></wc-info></entry></info>".into(),
            };Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout,stderr:String::new()})
        });
        let scope = Scope::new(host.invocation().project().unwrap(), "/second".into());
        (host, scope)
    }
    fn property(value: &str) -> String {
        format!(
            "<properties><target path=\"nested\"><property name=\"svn:ignore\">{value}</property></target></properties>"
        )
    }
    #[test]
    fn ignore_preparation_preserves_rules_and_rebases_only_the_attached_parent() {
        let (host, scope) = setup("unversioned", || property("keep\n"));
        host.answer_next_prompt(Some(&["yes"]));
        let args = prepare_ignore(&scope, &["nested/cache.tmp".into()], false, "test-client")
            .unwrap()
            .unwrap();
        let file = &args[2];
        assert_eq!(host::fs::read_file(file).unwrap(), "keep\ncache.tmp\n");
        let actual = scope.args(&args);
        assert_eq!(actual.last().unwrap(), "/second/nested@");
        assert_eq!(&actual[2], file);
        assert_eq!(actual[0], "propset");
    }
    #[test]
    fn unignore_removes_an_empty_property_and_leaves_global_rules_alone() {
        let (host, scope) = setup("ignored", || property("cache.tmp\n"));
        host.answer_next_prompt(Some(&["yes"]));
        let args = prepare_ignore(&scope, &["nested/cache.tmp".into()], true, "test-client")
            .unwrap()
            .unwrap();
        assert_eq!(args[0], "propdel");
        assert_eq!(args.last().unwrap(), "nested@");
        drop(host);
        let (_host, scope) = setup("ignored", || "<properties/>".into());
        assert!(
            prepare_ignore(&scope, &["nested/cache.tmp".into()], true, "test-client")
                .unwrap_err()
                .to_string()
                .contains("全局")
        );
    }
    #[test]
    fn concurrent_property_changes_are_rejected_after_confirmation() {
        let reads = Rc::new(Cell::new(0));
        let seen = reads.clone();
        let (host, scope) = setup("unversioned", move || {
            let n = seen.get();
            seen.set(n + 1);
            property(if n == 0 { "keep\n" } else { "changed\n" })
        });
        host.answer_next_prompt(Some(&["yes"]));
        assert!(
            prepare_ignore(&scope, &["nested/cache.tmp".into()], false, "test-client")
                .unwrap_err()
                .to_string()
                .contains("确认期间已变化")
        );
        assert_eq!(reads.get(), 2);
    }
    #[test]
    fn wildcard_unignore_is_explicitly_edited_and_confirmed() {
        let (host, scope) = setup("ignored", || property("*.tmp\nkeep\n"));
        host.answer_next_prompt(Some(&["keep\n"]));
        host.answer_next_prompt(Some(&["yes"]));
        let args = prepare_ignore(&scope, &["nested/cache.tmp".into()], true, "test-client")
            .unwrap()
            .unwrap();
        assert_eq!(host::fs::read_file(&args[2]).unwrap(), "keep\n");
    }

    #[test]
    fn inherited_ignore_does_not_edit_an_unrelated_local_rule() {
        let (_host, scope) = setup("ignored", || property("keep.txt\n"));
        assert!(
            prepare_ignore(&scope, &["nested/cache.tmp".into()], true, "test-client")
                .unwrap_err()
                .to_string()
                .contains("全局")
        );
    }
    #[test]
    fn overlapping_wildcard_still_matching_is_not_called_unignored() {
        let (host, scope) = setup("ignored", || property("cache.tmp\n*.tmp\n"));
        host.answer_next_prompt(Some(&["*.tmp\n"]));
        assert!(
            prepare_ignore(&scope, &["nested/cache.tmp".into()], true, "test-client")
                .unwrap_err()
                .to_string()
                .contains("仍")
        );
    }

    #[test]
    fn property_change_during_final_status_scan_is_not_overwritten() {
        let host =
            MockHost::install().with_capabilities(&["project.read", "process:svn", "storage"]);
        let scans = Rc::new(Cell::new(0));
        let seen = scans.clone();
        host.on_process("svn",move |args,_|{
            let stdout=match args[0].as_str(){
                "status"=>{seen.set(seen.get()+1);"<status><target path=\".\"><entry path=\"nested/cache.tmp\"><wc-status item=\"unversioned\" props=\"none\"/></entry></target></status>".into()},
                "proplist"=>property(if seen.get()<2{"keep\n"}else{"changed\n"}),
                _=>"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><wc-info><wcroot-abspath>/second</wcroot-abspath></wc-info></entry></info>".into(),
            };Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout,stderr:String::new()})
        });
        let scope = Scope::new(host.invocation().project().unwrap(), "/second".into());
        host.answer_next_prompt(Some(&["yes"]));
        assert!(
            prepare_ignore(&scope, &["nested/cache.tmp".into()], false, "test-client")
                .unwrap_err()
                .to_string()
                .contains("确认期间已变化")
        );
        assert_eq!(scans.get(), 2);
    }
}

#[cfg(test)]
mod revert_review_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn deleted_paths_recreated_on_disk_are_not_overwritten() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:svn"]);
        host.add_file("dir/file/keep", "untracked");
        host.add_dir("dir/file");
        let scope = Scope::new(host.invocation().project().unwrap(), "/project".into());
        let entries = [Entry {
            path: "dir/file".into(),
            status: "deleted".into(),
            kind: "file".into(),
            ..Default::default()
        }];
        assert!(selected_revert_paths(&scope, &entries, &["dir/file".into()]).is_err());
    }
}

fn check_deleted_paths<'a>(
    project: &Scope,
    entries: impl Iterator<Item = &'a Entry>,
) -> Result<(), Error> {
    for entry in entries.filter(|e| e.status == "deleted") {
        match project.project.stat(&project.absolute(&entry.path)) {
            Err(Error::NotFound(_)) => {}
            Err(e) => {
                return Err(error(format!(
                    "无法确认已删除路径 {} 的磁盘状态，请先检查本地内容后在 IDE 终端处理：{e}",
                    entry.path
                )));
            }
            Ok(_) => {
                return Err(error(format!(
                    "已删除路径 {} 在磁盘上仍有本地内容，直接还原可能覆盖它；请先将这些内容移出，再还原。",
                    entry.path
                )));
            }
        }
    }
    Ok(())
}
