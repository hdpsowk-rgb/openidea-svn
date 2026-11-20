use openidea_plugin::{ProcessOutput, mock::MockHost};
use openidea_svn::{plugin::SvnPlugin, protocol::Request};
use std::{fs, path::Path, process::Command};

fn svn(cwd: &Path, args: &[&str]) -> String {
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

fn dispatch(
    plugin: &mut SvnPlugin,
    host: &MockHost,
    root: &Path,
    action: &str,
    extra: serde_json::Value,
) -> Result<serde_json::Value, openidea_plugin::Error> {
    let mut value = serde_json::json!({"client":"history-test","generation":1,"root":root.to_str().unwrap(),"action":action});
    for (key, value_extra) in extra.as_object().unwrap() {
        value[key] = value_extra.clone();
    }
    plugin.dispatch(
        Request::parse(&value.to_string()).unwrap(),
        &host.invocation(),
    )
}

#[test]
fn copied_branch_history_diff_reads_the_copy_source_not_a_nonexistent_previous_branch() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .current_dir(root)
            .args(["create", "repo"])
            .status()
            .unwrap()
            .success()
    );
    let url = format!("file://{}", root.join("repo").display());
    svn(root, &["checkout", &url, "wc"]);
    let wc = root.join("wc");
    fs::create_dir(wc.join("trunk")).unwrap();
    for name in ["config.js", "modified.js", "deleted.js", "中文 @#.php"] {
        fs::write(wc.join("trunk").join(name), "source at r1\n").unwrap();
    }
    svn(&wc, &["add", "trunk"]);
    svn(&wc, &["commit", "-m", "source"]);
    fs::write(wc.join("trunk/config.js"), "source at r2\n").unwrap();
    svn(&wc, &["commit", "-m", "advance source"]);
    svn(
        &wc,
        &["copy", "-r", "1", &format!("{url}/trunk@1"), "branch"],
    );
    svn(&wc, &["delete", "branch/config.js"]);
    fs::write(wc.join("branch/config.js"), "replacement in branch\n").unwrap();
    svn(&wc, &["add", "branch/config.js"]);
    fs::write(wc.join("branch/modified.js"), "modified in branch\n").unwrap();
    fs::write(wc.join("branch/中文 @#.php"), "encoded path in branch\n").unwrap();
    svn(&wc, &["delete", "branch/deleted.js"]);
    svn(
        &wc,
        &[
            "copy",
            "-r",
            "1",
            &format!("{url}/trunk/config.js@1"),
            "branch/copied.js",
        ],
    );
    fs::write(wc.join("branch/copied.js"), "copied then edited\n").unwrap();
    fs::write(wc.join("branch/new.js"), "new file\n").unwrap();
    svn(&wc, &["add", "branch/new.js"]);
    svn(
        &wc,
        &[
            "copy",
            "-r",
            "2",
            &format!("{url}/trunk@2"),
            "branch/nested",
        ],
    );
    svn(&wc, &["delete", "branch/nested/config.js"]);
    fs::write(wc.join("branch/nested/config.js"), "nested replacement\n").unwrap();
    svn(&wc, &["add", "branch/nested/config.js"]);
    svn(&wc, &["commit", "-m", "copy branch and replace config"]);

    let logs = openidea_svn::svn::parse_log(&svn(&wc, &["log", "--xml", "-v", "-r", "3"])).unwrap();
    assert_eq!(
        logs[0]
            .paths
            .iter()
            .find(|p| p.path == "/branch/config.js")
            .unwrap()
            .action,
        "R"
    );
    assert_eq!(
        logs[0]
            .paths
            .iter()
            .find(|p| p.path == "/branch")
            .unwrap()
            .copy_from_revision,
        Some(1)
    );
    // The old implementation fails here with the same W160013/E200009 as the screenshot.
    let old_read = Command::new("svn")
        .args(["cat", "-r", "2", &format!("{url}/branch/config.js@2")])
        .output()
        .unwrap();
    assert!(!old_read.status.success());
    assert!(String::from_utf8_lossy(&old_read.stderr).contains("W160013"));
    // Peg the copy source at its original revision even if it is later removed.
    svn(&wc, &["update"]);
    svn(&wc, &["delete", "trunk"]);
    svn(&wc, &["commit", "-m", "remove source after copying"]);

    let host = MockHost::install().with_capabilities(&[
        "project.read",
        "editor.read",
        "storage",
        "process:svn",
    ]);
    host.set_project_root(Some(wc.to_str().unwrap()));
    let cwd = wc.clone();
    host.on_process("svn", move |args, _| {
        let output = Command::new("svn")
            .current_dir(&cwd)
            .args(args)
            .output()
            .unwrap();
        Ok(ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    });
    let mut plugin = SvnPlugin::default();
    dispatch(
        &mut plugin,
        &host,
        &wc,
        "log",
        serde_json::json!({"revision":3}),
    )
    .unwrap();
    for (path, left, right) in [
        (
            "/branch/config.js",
            "source at r1\n",
            "replacement in branch\n",
        ),
        (
            "/branch/modified.js",
            "source at r1\n",
            "modified in branch\n",
        ),
        ("/branch/deleted.js", "source at r1\n", ""),
        (
            "/branch/中文 @#.php",
            "source at r1\n",
            "encoded path in branch\n",
        ),
        (
            "/branch/copied.js",
            "source at r1\n",
            "copied then edited\n",
        ),
        ("/branch/new.js", "", "new file\n"),
        (
            "/branch/nested/config.js",
            "source at r2\n",
            "nested replacement\n",
        ),
    ] {
        dispatch(
            &mut plugin,
            &host,
            &wc,
            "history-diff",
            serde_json::json!({"revision":3,"path":path}),
        )
        .unwrap_or_else(|e| panic!("{path}: {e}"));
        let diffs = host.opened_diffs();
        let diff = &diffs.last().unwrap().0;
        assert_eq!(diff.left.as_deref(), Some(left), "{path}");
        assert_eq!(diff.right.as_deref(), Some(right), "{path}");
        if path == "/branch/config.js" {
            assert!(diff.left_label.as_deref().unwrap().contains("r1"));
            assert!(
                diff.left_label
                    .as_deref()
                    .unwrap()
                    .contains("/trunk/config.js")
            );
        }
    }

    let count = host.opened_diffs().len();
    host.on_process("svn", |_, _| {
        Ok(ProcessOutput {
            status: Some(1),
            stdout: String::new(),
            stderr: "svn: E170001: authorization failed".into(),
        })
    });
    let err = dispatch(
        &mut plugin,
        &host,
        &wc,
        "history-diff",
        serde_json::json!({"revision":3,"path":"/branch/config.js"}),
    )
    .unwrap_err();
    assert!(err.to_string().contains("E170001"));
    assert_eq!(
        host.opened_diffs().len(),
        count,
        "failed content reads must not open a partial diff"
    );
}

#[test]
fn ordinary_history_replacements_use_the_previous_target_even_with_their_own_copy_source() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("svnadmin")
            .current_dir(root)
            .args(["create", "repo"])
            .status()
            .unwrap()
            .success()
    );
    let url = format!("file://{}", root.join("repo").display());
    svn(root, &["checkout", &url, "wc"]);
    let wc = root.join("wc");
    for (name, text) in [
        ("replaced.js", "old target\n"),
        ("source.js", "different source\n"),
        ("modified.js", "before\n"),
        ("deleted.js", "removed\n"),
    ] {
        fs::write(wc.join(name), text).unwrap();
        svn(&wc, &["add", name]);
    }
    svn(&wc, &["commit", "-m", "initial"]);
    svn(&wc, &["delete", "replaced.js"]);
    svn(&wc, &["copy", "source.js", "replaced.js"]);
    fs::write(wc.join("modified.js"), "after\n").unwrap();
    svn(&wc, &["delete", "deleted.js"]);
    fs::write(wc.join("new.js"), "new\n").unwrap();
    svn(&wc, &["add", "new.js"]);
    svn(&wc, &["copy", "source.js", "copied.js"]);
    svn(&wc, &["commit", "-m", "regular changes"]);
    let host = MockHost::install().with_capabilities(&[
        "project.read",
        "editor.read",
        "storage",
        "process:svn",
    ]);
    host.set_project_root(Some(wc.to_str().unwrap()));
    let cwd = wc.clone();
    host.on_process("svn", move |args, _| {
        let output = Command::new("svn")
            .current_dir(&cwd)
            .args(args)
            .output()
            .unwrap();
        Ok(ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    });
    let mut plugin = SvnPlugin::default();
    dispatch(
        &mut plugin,
        &host,
        &wc,
        "log",
        serde_json::json!({"revision":2}),
    )
    .unwrap();
    for (path, left, right) in [
        ("/replaced.js", "old target\n", "different source\n"),
        ("/modified.js", "before\n", "after\n"),
        ("/deleted.js", "removed\n", ""),
        ("/new.js", "", "new\n"),
        ("/copied.js", "different source\n", "different source\n"),
    ] {
        dispatch(
            &mut plugin,
            &host,
            &wc,
            "history-diff",
            serde_json::json!({"revision":2,"path":path}),
        )
        .unwrap();
        let diffs = host.opened_diffs();
        let diff = &diffs.last().unwrap().0;
        assert_eq!(diff.left.as_deref(), Some(left), "{path}");
        assert_eq!(diff.right.as_deref(), Some(right), "{path}");
    }
}
