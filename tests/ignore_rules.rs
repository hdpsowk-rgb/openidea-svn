use openidea_svn::{ignore_rules as rules, svn};
use std::{fs, path::Path, process::Command};

#[test]
fn ignore_values_preserve_other_rules_and_escape_literal_names() {
    assert_eq!(
        rules::update(Some("*.tmp\ncache/\n"), "a[*]?.log", false).unwrap(),
        Some("*.tmp\ncache/\na[[][*][]][?].log\n".into())
    );
    assert_eq!(
        rules::update(Some("keep\r\nfoo\r\n"), "foo", true).unwrap(),
        Some("keep\r\n".into())
    );
    assert_eq!(rules::update(Some("foo\n"), "foo", true).unwrap(), None);
    assert_eq!(
        rules::update(Some(" \nfoo\n"), "foo", true).unwrap(),
        Some(" \n".into())
    );
    assert_eq!(
        rules::update(Some("foo\n"), "foo", false).unwrap(),
        Some("foo\n".into())
    );
    assert!(rules::update(Some("*.log\n"), "test.log", true).is_err());
    for invalid in [".", "..", "a/b", "a\\b", "a\nb", ""] {
        assert!(rules::update(None, invalid, false).is_err());
    }
    assert_eq!(rules::parse("<properties><target path=\".\"><property name=\"other\">keep</property><property name=\"svn:ignore\">a&amp;b\n</property></target></properties>").unwrap(),Some("a&b\n".into()));
    assert_eq!(rules::parse("<properties/>").unwrap(), None);
    assert!(rules::parse("<properties>").is_err());
}
fn run(cwd: &Path, args: &[String]) -> String {
    let output = Command::new("svn")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn actual_svn_ignore_and_unignore_keep_files_and_unrelated_rules() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .args(["create", "repo"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    run(
        root,
        &[
            "co".into(),
            format!("file://{}/repo", root.display()),
            "wc".into(),
        ],
    );
    let wc = root.join("wc");
    let name = "-a[*]?中文@.log";
    fs::write(wc.join(name), "local").unwrap();
    fs::write(wc.join("other.log"), "other").unwrap();
    let original = "*.tmp\n";
    let value = rules::update(Some(original), name, false).unwrap().unwrap();
    let file = root.join("ignore.txt");
    fs::write(&file, value).unwrap();
    run(
        &wc,
        &rules::command(".", Some(file.to_str().unwrap())).unwrap(),
    );
    let status =
        svn::parse_status(&run(&wc, &svn::command("status", &[], "", None).unwrap())).unwrap();
    assert!(!status.iter().any(|e| e.path == name));
    assert!(status.iter().any(|e| e.path == "other.log"));
    assert_eq!(fs::read_to_string(wc.join(name)).unwrap(), "local");
    let read = rules::parse(&run(
        &wc,
        &svn::command("ignore-list", &[".".into()], "", None).unwrap(),
    ))
    .unwrap();
    fs::write(
        &file,
        rules::update(read.as_deref(), name, true).unwrap().unwrap(),
    )
    .unwrap();
    run(
        &wc,
        &rules::command(".", Some(file.to_str().unwrap())).unwrap(),
    );
    assert_eq!(
        rules::parse(&run(
            &wc,
            &svn::command("ignore-list", &[".".into()], "", None).unwrap()
        ))
        .unwrap()
        .as_deref(),
        Some(original)
    );
    assert!(
        svn::parse_status(&run(&wc, &svn::command("status", &[], "", None).unwrap()))
            .unwrap()
            .iter()
            .any(|e| e.path == name)
    );
    run(&wc, &rules::command(".", None).unwrap());
    assert_eq!(
        rules::parse(&run(
            &wc,
            &svn::command("ignore-list", &[".".into()], "", None).unwrap()
        ))
        .unwrap(),
        None
    );
}

#[test]
fn actual_recursive_revert_restores_tracked_tree_and_keeps_untracked_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .args(["create", "repo"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    run(
        root,
        &[
            "co".into(),
            format!("file://{}/repo", root.display()),
            "wc".into(),
        ],
    );
    let wc = root.join("wc");
    fs::create_dir(wc.join("dir")).unwrap();
    fs::write(wc.join("dir/a"), "base").unwrap();
    run(&wc, &["add".into(), "dir".into()]);
    run(&wc, &["ci".into(), "-m".into(), "Create baseline".into()]);
    fs::write(wc.join("dir/a"), "modified").unwrap();
    fs::write(wc.join("dir/keep.tmp"), "untracked").unwrap();
    run(
        &wc,
        &svn::command("revert-tree", &["dir".into()], "", None).unwrap(),
    );
    assert_eq!(fs::read_to_string(wc.join("dir/a")).unwrap(), "base");
    assert_eq!(
        fs::read_to_string(wc.join("dir/keep.tmp")).unwrap(),
        "untracked"
    );
    run(&wc, &["delete".into(), "--force".into(), "dir".into()]);
    run(
        &wc,
        &svn::command("revert-tree", &["dir".into()], "", None).unwrap(),
    );
    assert_eq!(fs::read_to_string(wc.join("dir/a")).unwrap(), "base");
    fs::create_dir(wc.join("new")).unwrap();
    fs::write(wc.join("new/file"), "new").unwrap();
    run(&wc, &["add".into(), "new".into()]);
    let entries =
        svn::parse_status(&run(&wc, &svn::command("status", &[], "", None).unwrap())).unwrap();
    let paths = svn::order_revert(&entries, &["new".into(), "new/file".into()]).unwrap();
    assert_eq!(paths, ["new/file", "new"]);
    run(&wc, &svn::command("revert", &paths, "", None).unwrap());
    assert_eq!(fs::read_to_string(wc.join("new/file")).unwrap(), "new");
}

#[test]
fn copied_directory_revert_blocks_before_deleting_untracked_children() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .args(["create", "repo"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    run(
        root,
        &[
            "co".into(),
            format!("file://{}/repo", root.display()),
            "wc".into(),
        ],
    );
    let wc = root.join("wc");
    fs::create_dir(wc.join("base")).unwrap();
    fs::write(wc.join("base/a"), "base").unwrap();
    run(&wc, &["add".into(), "base".into()]);
    run(&wc, &["ci".into(), "-m".into(), "Create baseline".into()]);
    run(&wc, &["copy".into(), "base".into(), "copy".into()]);
    fs::write(wc.join("copy/keep.tmp"), "untracked").unwrap();
    let entries = svn::parse_status(&run(
        &wc,
        &["status".into(), "--xml".into(), "--no-ignore".into()],
    ))
    .unwrap();
    assert!(
        svn::validate_revert_tree(&entries, "copy").is_err(),
        "unsafe copied-directory revert was accepted"
    );
    assert_eq!(
        fs::read_to_string(wc.join("copy/keep.tmp")).unwrap(),
        "untracked"
    );
}

#[test]
fn destructive_revert_guards_cover_hidden_metadata_and_changelists() {
    let entries=svn::parse_status(r#"<status><target path="."><entry path="copy"><wc-status item="added" copied="true" props="none"/></entry><entry path="copy/.git"><wc-status item="unversioned" props="none"/></entry></target></status>"#).unwrap();
    assert!(svn::validate_revert_tree(&entries, ".").is_err());
    assert!(svn::order_revert(&entries, &["copy".into()]).is_err());
    let entries=svn::parse_status(r#"<status><target path="."/><changelist name="work"><entry path="dir/file"><wc-status item="obstructed" props="none"/></entry></changelist></status>"#).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(svn::validate_revert_tree(&entries, "dir").is_err());
    assert!(svn::order_revert(&entries, &["dir/file".into()]).is_err());
}
#[test]
fn matching_ignore_patterns_identify_overlapping_and_unrelated_rules() {
    assert!(rules::matches_any("*.tmp\ncache.tmp\n", "cache.tmp"));
    assert!(!rules::matches_any("keep.txt\n", "example.log"));
    assert!(rules::matches_any("[[]cache[]]\n", "[cache]"));
    assert!(rules::matches_any("[!a-z]*\n", "1file"));
    assert!(!rules::matches_any("[!a-z]*\n", "afile"));
}

#[test]
fn utf8_ignore_properties_do_not_depend_on_the_gui_locale() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .args(["create", "repo"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    run(
        root,
        &[
            "co".into(),
            format!("file://{}/repo", root.display()),
            "wc".into(),
        ],
    );
    let wc = root.join("wc");
    let file = root.join("rules.txt");
    fs::write(&file, "中文.txt\n").unwrap();
    let output = Command::new("svn")
        .current_dir(&wc)
        .args(rules::command(".", Some(file.to_str().unwrap())).unwrap())
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = rules::parse(&run(
        &wc,
        &svn::command("ignore-list", &[".".into()], "", None).unwrap(),
    ))
    .unwrap();
    assert_eq!(actual.as_deref(), Some("中文.txt\n"));
}

#[test]
fn ignore_globs_agree_with_the_local_svn_client() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .args(["create", "repo"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    run(
        root,
        &[
            "co".into(),
            format!("file://{}/repo", root.display()),
            "wc".into(),
        ],
    );
    let wc = root.join("wc");
    let names = [
        "cache.tmp",
        "fooa.txt",
        "abc",
        "[cache]",
        "a*.log",
        ".hidden",
        "中.txt",
        "space name.tmp",
        "[bad",
        "[]",
        "[!bad",
    ];
    for name in names {
        fs::write(wc.join(name), "local").unwrap();
    }
    let file = root.join("patterns");
    let config = root.join("config");
    fs::create_dir(&config).unwrap();
    for pattern in [
        "*.tmp",
        "foo?.txt",
        "[a-z]*",
        "[!a-z]*",
        "[[]cache[]]",
        "a\\*.log",
        "*",
        "?.txt",
        "???.txt",
        "space name.tmp",
        "[bad",
        "[]",
        "[!bad",
    ] {
        let value = format!("{pattern}\n");
        fs::write(&file, &value).unwrap();
        run(
            &wc,
            &rules::command(".", Some(file.to_str().unwrap())).unwrap(),
        );
        let entries = svn::parse_status(&run(
            &wc,
            &[
                "status".into(),
                "--xml".into(),
                "--no-ignore".into(),
                "--config-dir".into(),
                config.to_str().unwrap().into(),
                "--config-option".into(),
                "config:miscellany:global-ignores=".into(),
            ],
        ))
        .unwrap();
        for name in names {
            let actual = entries
                .iter()
                .any(|e| e.path == name && e.status == "ignored");
            assert_eq!(
                rules::matches_any(&value, name),
                actual,
                "pattern={pattern:?}, name={name:?}"
            );
        }
    }
}
