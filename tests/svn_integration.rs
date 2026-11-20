use openidea_svn::svn::{self, Entry};
use std::{fs, path::Path, process::Command};

fn entry(path: &str, status: &str, kind: &str) -> Entry {
    Entry {
        path: path.into(),
        status: status.into(),
        kind: kind.into(),
        metadata_complete: kind == "dir",
        ..Default::default()
    }
}

#[test]
fn literal_targets_cannot_escape_or_become_options() {
    assert_eq!(svn::target("中文 空格@x.txt").unwrap(), "中文 空格@x.txt@");
    assert_eq!(svn::target("-option.txt").unwrap(), "-option.txt@");
    for invalid in [
        "../outside",
        "/absolute",
        "a/../../b",
        "",
        "a\0b",
        "a\nb",
        "http://host/a",
        "C:\\outside",
    ] {
        assert!(svn::target(invalid).is_err(), "accepted {invalid:?}");
    }
    let args = svn::command("add", &["-x@中文".into()], "", None).unwrap();
    assert_eq!(&args[args.len() - 2..], &["--", "-x@中文@"]);
    assert!(args.contains(&"empty".into()));
    assert!(svn::command("arbitrary", &[], "", None).is_err());
}

#[test]
fn parses_status_properties_conflicts_and_xml_entities() {
    let xml = r#"<?xml version="1.0"?><status><target path="."><entry path="中文 &amp; 空格@.txt"><wc-status item="modified" props="modified" tree-conflicted="true" revision="2"/></entry><entry path="ignored"><wc-status item="external" props="none"/></entry></target></status>"#;
    let entries = svn::parse_status(xml).unwrap();
    assert_eq!(entries[0].path, "中文 & 空格@.txt");
    assert_eq!(entries[0].props, "modified");
    assert!(entries[0].tree_conflicted);
    assert_eq!(entries[1].status, "external");
    assert!(svn::parse_status("truncated <status>").is_err());
}

#[test]
fn parses_info_and_history_without_losing_multiline_messages() {
    let info = svn::parse_info(r#"<info><entry kind="dir" path="." revision="12"><url>file:///repo/trunk</url><repository><root>file:///repo</root><uuid>x</uuid></repository><wc-info><wcroot-abspath>/tmp/wc</wcroot-abspath></wc-info></entry></info>"#).unwrap();
    assert_eq!(info.revision, 12);
    assert_eq!(info.wc_root, "/tmp/wc");
    let logs = svn::parse_log("<log><logentry revision=\"9\"><author>作者</author><date>2026-11-17T00:00:00Z</date><paths><path action=\"M\" kind=\"file\">/trunk/a</path></paths><msg>first\nsecond &amp; more</msg></logentry></log>").unwrap();
    assert_eq!(logs[0].message, "first\nsecond & more");
    assert_eq!(logs[0].paths[0].action, "M");
    assert!(svn::parse_info("<info/>").is_err());
}

#[test]
fn commit_requires_explicit_safe_complete_targets() {
    let entries = vec![
        entry("a", "modified", "file"),
        entry("b", "modified", "file"),
    ];
    svn::validate_commit(&entries, &["a".into()]).unwrap();
    assert!(svn::validate_commit(&entries, &[]).is_err());
    assert!(svn::validate_commit(&entries, &["unknown".into()]).is_err());
    for status in [
        "conflicted",
        "unversioned",
        "external",
        "missing",
        "obstructed",
        "incomplete",
    ] {
        assert!(svn::validate_commit(&[entry("a", status, "file")], &["a".into()]).is_err());
    }
    let deleted = vec![
        entry("dir", "deleted", "dir"),
        entry("dir/child", "deleted", "file"),
    ];
    assert!(svn::validate_commit(&deleted, &["dir".into()]).is_err());
    svn::validate_commit(&deleted, &["dir".into(), "dir/child".into()]).unwrap();
    let added = vec![
        entry("dir", "added", "dir"),
        entry("dir/child", "added", "file"),
    ];
    assert!(svn::validate_commit(&added, &["dir/child".into()]).is_err());
    let args = svn::command("commit", &["a".into()], "message\n中文", None).unwrap();
    assert!(args.windows(2).any(|a| a == ["--depth", "empty"]));
    assert!(svn::command("commit", &["a".into()], "  ", None).is_err());
}

fn run(cwd: &Path, binary: &str, args: &[String]) -> String {
    let out = Command::new(binary)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{binary} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn raw(cwd: &Path, args: &[&str]) -> String {
    run(
        cwd,
        "svn",
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
}

#[test]
fn chinese_commit_messages_are_utf8_despite_native_locale_and_log_encoding_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, "svnadmin", &["create".into(), "repo".into()]);
    let url = format!("file://{}", root.join("repo").display());
    raw(root, &["checkout", &url, "wc"]);
    let wc = root.join("wc");
    fs::write(wc.join("a.txt"), "initial\n").unwrap();
    raw(&wc, &["add", "a.txt"]);
    let config = root.join("config");
    fs::create_dir(&config).unwrap();
    let message = "前端\n修复充值、提现与配置。Emoji 🙂\n引号：\" ' & < > @ % -- 不是参数";
    let mut scenarios = vec![
        ("legacy log-encoding config", "C", "ISO-8859-1"),
        ("ASCII GUI locale", "C", ""),
    ];
    if cfg!(target_os = "macos") {
        scenarios.push(("Latin-1 GUI locale", "en_US.ISO8859-1", ""));
    }
    for (index, (name, locale, encoding)) in scenarios.into_iter().enumerate() {
        fs::write(
            config.join("config"),
            format!("[miscellany]\nlog-encoding = {encoding}\n"),
        )
        .unwrap();
        fs::write(wc.join("a.txt"), format!("change {index}\n")).unwrap();
        let mut args = svn::command("commit", &["a.txt".into()], message, None).unwrap();
        args.splice(
            1..1,
            ["--config-dir".into(), config.to_str().unwrap().into()],
        );
        let output = Command::new("svn")
            .current_dir(&wc)
            .args(&args)
            .env("LC_ALL", locale)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Inspect the actual revision property, not just the panel's input or output.
        let logs =
            svn::parse_log(&raw(&wc, &["log", "--xml", "-r", &(index + 1).to_string()])).unwrap();
        assert_eq!(
            logs[0].message, message,
            "{name}: repository log text was corrupted"
        );
        assert_eq!(
            raw(root, &["cat", &format!("{url}/a.txt")]),
            format!("change {index}\n")
        );
    }
}

#[test]
fn real_repository_selected_commit_update_revert_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, "svnadmin", &["create".into(), "repo".into()]);
    let url = format!("file://{}", root.join("repo").display());
    raw(root, &["checkout", &url, "wc"]);
    raw(root, &["checkout", &url, "other"]);
    let wc = root.join("wc");
    let a = "中文 空格@文件.txt";
    let b = "-option.txt";
    fs::write(wc.join(a), "base\n").unwrap();
    fs::write(wc.join(b), "untouched\n").unwrap();
    for path in [a, b] {
        run(
            &wc,
            "svn",
            &svn::command("add", &[path.into()], "", None).unwrap(),
        );
    }
    let entries = svn::parse_status(&run(
        &wc,
        "svn",
        &svn::command("status", &[], "", None).unwrap(),
    ))
    .unwrap();
    svn::validate_commit(&entries, &[a.into()]).unwrap();
    run(
        &wc,
        "svn",
        &svn::command("commit", &[a.into()], "只提交中文文件", None).unwrap(),
    );
    let status = svn::parse_status(&raw(&wc, &["status", "--xml"])).unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].path, b);
    assert_eq!(status[0].status, "added");
    fs::write(wc.join(a), "modified\n").unwrap();
    assert_eq!(
        run(
            &wc,
            "svn",
            &svn::command("cat-base", &[a.into()], "", None).unwrap()
        ),
        "base\n"
    );
    run(
        &wc,
        "svn",
        &svn::command("revert", &[a.into()], "", None).unwrap(),
    );
    assert_eq!(fs::read_to_string(wc.join(a)).unwrap(), "base\n");
    let logs = svn::parse_log(&run(
        &wc,
        "svn",
        &svn::command("log", &[], "", None).unwrap(),
    ))
    .unwrap();
    assert_eq!(logs[0].message, "只提交中文文件");
    let other = root.join("other");
    run(
        &other,
        "svn",
        &svn::command("update", &[], "", None).unwrap(),
    );
    assert_eq!(fs::read_to_string(other.join(a)).unwrap(), "base\n");
    assert!(!other.join(b).exists());
}

#[test]
fn real_repository_conflicts_are_detected_and_resolved_explicitly() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, "svnadmin", &["create".into(), "repo".into()]);
    let url = format!("file://{}", root.join("repo").display());
    raw(root, &["co", &url, "wc"]);
    let wc = root.join("wc");
    fs::write(wc.join("a"), "base\n").unwrap();
    raw(&wc, &["add", "a"]);
    raw(&wc, &["ci", "-m", "initial"]);
    raw(root, &["co", &url, "other"]);
    let other = root.join("other");
    fs::write(wc.join("a"), "theirs\n").unwrap();
    raw(&wc, &["ci", "-m", "remote"]);
    fs::write(other.join("a"), "mine\n").unwrap();
    run(
        &other,
        "svn",
        &svn::command("update", &[], "", None).unwrap(),
    );
    let entries = svn::parse_status(&raw(&other, &["status", "--xml"])).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == "a" && e.status == "conflicted")
    );
    assert!(svn::validate_commit(&entries, &["a".into()]).is_err());
    run(
        &other,
        "svn",
        &svn::command("resolve-mine", &["a".into()], "", None).unwrap(),
    );
    assert_eq!(fs::read_to_string(other.join("a")).unwrap(), "mine\n");
    let entries = svn::parse_status(&raw(&other, &["status", "--xml"])).unwrap();
    svn::validate_commit(&entries, &["a".into()]).unwrap();
}

#[test]
fn deleted_directories_are_expanded_before_selective_commit() {
    let mut entries = vec![entry("dir", "deleted", "unknown")];
    assert!(svn::validate_commit(&entries, &["dir".into()]).is_err());
    svn::enrich_info(&mut entries,r#"<info><entry path="dir" kind="dir" revision="1"><wc-info><schedule>delete</schedule></wc-info></entry><entry path="dir/a" kind="file" revision="1"><wc-info><schedule>delete</schedule></wc-info></entry></info>"#).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind, "dir");
    assert!(svn::validate_commit(&entries, &["dir".into()]).is_err());
    svn::validate_commit(&entries, &["dir".into(), "dir/a".into()]).unwrap();
}

#[test]
fn binary_non_utf8_and_oversized_files_are_not_misrepresented_as_text() {
    assert_eq!(svn::text_content("中文".as_bytes()).unwrap(), "中文");
    assert!(svn::text_content(b"a\0b").is_err());
    assert!(svn::text_content(&[0xff, 0xfe]).is_err());
    assert!(svn::text_content(&vec![b'a'; 1024 * 1024 + 1]).is_err());
}

#[test]
fn real_deleted_directory_commit_includes_only_explicit_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, "svnadmin", &["create".into(), "repo".into()]);
    let url = format!("file://{}", root.join("repo").display());
    raw(root, &["co", &url, "wc"]);
    let wc = root.join("wc");
    fs::create_dir(wc.join("dir")).unwrap();
    fs::write(wc.join("dir/a"), "a").unwrap();
    fs::write(wc.join("other"), "base").unwrap();
    raw(&wc, &["add", "dir", "other"]);
    raw(&wc, &["ci", "-m", "seed"]);
    fs::write(wc.join("other"), "keep local").unwrap();
    run(
        &wc,
        "svn",
        &svn::command("delete", &["dir".into()], "", None).unwrap(),
    );
    let mut entries = svn::parse_status(&raw(&wc, &["status", "--xml"])).unwrap();
    svn::enrich_info(
        &mut entries,
        &run(
            &wc,
            "svn",
            &svn::command("info-tree", &["dir".into()], "", None).unwrap(),
        ),
    )
    .unwrap();
    assert!(svn::validate_commit(&entries, &["dir".into()]).is_err());
    let selected = vec!["dir".into(), "dir/a".into()];
    svn::validate_commit(&entries, &selected).unwrap();
    run(
        &wc,
        "svn",
        &svn::command("commit", &selected, "delete selected tree", None).unwrap(),
    );
    let log = svn::parse_log(&raw(&wc, &["log", "--xml", "-v", "-r", "HEAD"])).unwrap();
    assert!(!log[0].paths.iter().any(|p| p.path == "/other"));
    let entries = svn::parse_status(&raw(&wc, &["status", "--xml"])).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == "other" && e.status == "modified")
    );
}

#[test]
fn selecting_project_root_cannot_recursively_commit_unchecked_children() {
    let entries = vec![entry(".", "deleted", "dir"), entry("a", "deleted", "file")];
    assert!(svn::validate_commit(&entries, &[".".into()]).is_err());
    svn::validate_commit(&entries, &[".".into(), "a".into()]).unwrap();
}

#[test]
fn modified_file_externals_remain_outside_commit_selection() {
    let xml = r#"<status><target path="."><entry path="ext"><wc-status item="modified" props="none" file-external="true"/></entry></target></status>"#;
    let entries = svn::parse_status(xml).unwrap();
    assert_eq!(entries[0].status, "external");
    assert!(svn::validate_commit(&entries, &["ext".into()]).is_err());
}

#[test]
fn replacing_a_directory_with_a_file_requires_a_known_base_tree() {
    let mut replacement = entry("dir", "replaced", "file");
    replacement.metadata_complete = true;
    assert!(svn::validate_commit(&[replacement], &["dir".into()]).is_err());
}

#[test]
fn attached_directory_absolute_targets_keep_changes_and_commit_in_that_directory() {
    use openidea_plugin::mock::MockHost;
    use openidea_svn::workspace::Scope;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, "svnadmin", &["create".into(), "repo".into()]);
    let url = format!("file://{}", root.join("repo").display());
    raw(root, &["co", &url, "first"]);
    raw(root, &["co", &url, "第二项目"]);
    let first = root.join("first");
    let second = root.join("第二项目");
    fs::write(first.join("a.txt"), "base\n").unwrap();
    raw(&first, &["add", "a.txt"]);
    raw(&first, &["ci", "-m", "initial"]);
    raw(&second, &["up"]);
    fs::write(first.join("a.txt"), "first stays\n").unwrap();
    fs::write(second.join("a.txt"), "second chosen\n").unwrap();
    fs::write(second.join("other.txt"), "unselected\n").unwrap();
    raw(&second, &["add", "other.txt"]);
    let host = MockHost::install();
    host.set_project_root(first.to_str());
    let scope = Scope::new(
        host.invocation().project().unwrap(),
        second.to_str().unwrap().into(),
    );
    assert!(!scope.direct);
    let xml = run(
        &first,
        "svn",
        &scope.args(&svn::command("status", &[], "", None).unwrap()),
    );
    let entries = svn::parse_status_at(&xml, Some(&scope.root)).unwrap();
    assert!(entries.iter().any(|e| e.path == "a.txt"));
    assert!(entries.iter().any(|e| e.path == "other.txt"));
    assert!(svn::parse_status_at(&xml, Some(first.to_str().unwrap())).is_err());
    let message = "literal -- /not/a/path@";
    let args = scope.args(&svn::command("commit", &["a.txt".into()], message, None).unwrap());
    assert!(args.iter().any(|a| a == message));
    run(&first, "svn", &args);
    assert!(raw(&first, &["status", "--xml"]).contains("modified"));
    let remaining = svn::parse_status(&raw(&second, &["status", "--xml"])).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].path, "other.txt");
    assert_eq!(
        raw(root, &["cat", &format!("{url}/a.txt")]),
        "second chosen\n"
    );
}

#[test]
fn windows_status_and_metadata_use_safe_relative_paths() {
    let root = r"D:\workspace\中文 示例项目";
    let xml = r#"<status><target path="."><entry path="D:/workspace/中文 示例项目/src\中文.js"><wc-status item="modified" props="none"/></entry><entry path=".\deleted\a.txt"><wc-status item="deleted" props="none"/></entry></target></status>"#;
    let mut entries = svn::parse_status_at(xml, Some(root)).unwrap();
    assert_eq!(entries[0].path, "src/中文.js");
    assert_eq!(entries[1].path, "deleted/a.txt");
    let info = r#"<info><entry path="d:\WORKSPACE\中文 示例项目\deleted\a.txt" kind="file"><wc-info><schedule>delete</schedule></wc-info></entry></info>"#;
    svn::enrich_info_at(&mut entries, info, Some(root)).unwrap();
    assert_eq!(entries[1].kind, "file");
    assert!(entries[1].metadata_complete);
    let unc = r#"<status><target path="."><entry path="\\server\share\Project\src\a.txt"><wc-status item="modified"/></entry></target></status>"#;
    assert_eq!(
        svn::parse_status_at(unc, Some("//SERVER/share/project")).unwrap()[0].path,
        "src/a.txt"
    );
}

#[test]
fn windows_normalization_still_rejects_foreign_and_unsafe_targets() {
    for path in [
        r"E:\repo\a.txt",
        r"D:\repo-other\a.txt",
        r"D:\repo\..\outside.txt",
        r"D:\repo\.svn\wc.db",
        r"\outside.txt",
    ] {
        let xml = format!(
            r#"<status><target path="."><entry path="{path}"><wc-status item="modified"/></entry></target></status>"#
        );
        assert!(
            svn::parse_status_at(&xml, Some("D:/repo")).is_err(),
            "accepted {path}"
        );
    }
    let unix = r#"<status><target path="."><entry path="dir\not-a-separator"><wc-status item="modified"/></entry></target></status>"#;
    assert!(svn::parse_status_at(unix, Some("/repo")).is_err());
}

#[test]
fn unversioned_git_metadata_does_not_hide_valid_svn_changes() {
    let xml = r#"<status><target path="."><entry path=".git"><wc-status item="unversioned"/></entry><entry path="src/a.txt"><wc-status item="modified"/></entry></target></status>"#;
    let entries = svn::parse_status_at(xml, Some("/repo")).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "src/a.txt");
    assert!(svn::target(".git/config").is_err());
}
