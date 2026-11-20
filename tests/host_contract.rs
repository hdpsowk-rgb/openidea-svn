use openidea_plugin::mock::MockHost;
use openidea_svn::{plugin::SvnPlugin, protocol::Request};

fn request(action: &str, root: Option<&str>, client: &str) -> Request {
    Request::parse(
        &serde_json::json!({"client":client,"generation":1,"root":root,"action":action})
            .to_string(),
    )
    .unwrap()
}
#[test]
fn handshake_binds_to_invoking_project_and_foreign_roots_fail() {
    let host = MockHost::install();
    host.set_project_root(Some("/project"));
    let mut plugin = SvnPlugin::default();
    let result = plugin
        .dispatch(request("hello", None, "one"), &host.invocation())
        .unwrap();
    assert_eq!(result["root"], "/project");
    assert_eq!(result["client"], "one");
    assert_eq!(result["busy"], false);
    assert!(
        plugin
            .dispatch(
                request("refresh", Some("/other"), "one"),
                &host.invocation()
            )
            .is_err()
    );
}
#[test]
fn authentication_uses_ide_terminal_without_passing_credentials() {
    let host = MockHost::install().with_capabilities(&["project.read", "process:svn", "storage"]);
    let mut plugin = SvnPlugin::default();
    plugin
        .dispatch(
            request("authenticate", Some("/project"), "one"),
            &host.invocation(),
        )
        .unwrap();
    let terminals = host.terminals();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].program, "svn");
    assert!(
        !terminals[0]
            .args
            .iter()
            .any(|a| a == "--non-interactive" || a == "--password")
    );
}
#[test]
fn writes_are_serialized_across_panels_and_cancellation_releases_the_lock() {
    let host = MockHost::install().with_capabilities(&[
        "project.read",
        "process:svn",
        "editor.read",
        "storage",
    ]);
    let mut plugin = SvnPlugin::default();
    let result = plugin
        .dispatch(
            request("update", Some("/project"), "one"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(result["busy"], true);
    assert!(
        plugin
            .dispatch(
                request("cleanup", Some("/project"), "two"),
                &host.invocation()
            )
            .is_err()
    );
    let result = plugin
        .dispatch(
            request("cancel", Some("/project"), "one"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(result["busy"], false);
    plugin
        .dispatch(
            request("cleanup", Some("/project"), "two"),
            &host.invocation(),
        )
        .unwrap();
}
#[test]
fn update_rejects_unsaved_editors_before_starting_a_process() {
    let host = MockHost::install();
    host.open_editor("/project/a", "text", "saved", &[]);
    host.type_in_editor("unsaved");
    let mut plugin = SvnPlugin::default();
    let result = plugin.dispatch(
        request("update", Some("/project"), "one"),
        &host.invocation(),
    );
    assert!(result.unwrap_err().to_string().contains("未保存"));
}

fn metadata_host() -> MockHost {
    let host = MockHost::install().with_capabilities(&[
        "project.read",
        "editor.read",
        "storage",
        "process:svn",
    ]);
    host.on_process("svn",|args,_|{
        let xml=if args[0]=="status" {"<status><target path=\".\"/></status>"}else{"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><repository><root>file:///repo</root></repository><wc-info><wcroot-abspath>/project</wcroot-abspath></wc-info></entry></info>"};
        Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout:xml.into(),stderr:String::new()})
    });
    host
}
#[test]
fn projects_inside_one_working_copy_share_the_write_lock() {
    let host = metadata_host();
    let mut plugin = SvnPlugin::default();
    host.set_project_root(Some("/project/one"));
    plugin
        .dispatch(
            request("refresh", Some("/project/one"), "one"),
            &host.invocation(),
        )
        .unwrap();
    host.set_project_root(Some("/project/two"));
    plugin
        .dispatch(
            request("refresh", Some("/project/two"), "two"),
            &host.invocation(),
        )
        .unwrap();
    host.set_project_root(Some("/project/one"));
    plugin
        .dispatch(
            request("update", Some("/project/one"), "one"),
            &host.invocation(),
        )
        .unwrap();
    host.set_project_root(Some("/project/two"));
    assert!(
        plugin
            .dispatch(
                request("update", Some("/project/two"), "two"),
                &host.invocation()
            )
            .is_err()
    );
}

#[test]
fn a_fresh_panel_in_a_busy_working_copy_cannot_bypass_the_shared_lock() {
    let host = metadata_host();
    let mut plugin = SvnPlugin::default();
    host.set_project_root(Some("/project/one"));
    plugin
        .dispatch(
            request("refresh", Some("/project/one"), "one"),
            &host.invocation(),
        )
        .unwrap();
    plugin
        .dispatch(
            request("update", Some("/project/one"), "one"),
            &host.invocation(),
        )
        .unwrap();
    host.set_project_root(Some("/project/two"));
    let result = plugin.dispatch(
        request("update", Some("/project/two"), "new-panel"),
        &host.invocation(),
    );
    assert!(result.unwrap_err().to_string().contains("另一个 SVN 面板"));
}

#[test]
fn editor_menu_bare_path_argument_is_kept_when_selection_list_is_empty() {
    use openidea_plugin::Plugin;
    let host = metadata_host();
    let mut plugin = SvnPlugin::default();
    plugin
        .run_command("log", &host.invocation(), Some("/project/one.txt"))
        .unwrap();
    let response = plugin
        .dispatch(
            request("refresh", Some("/project"), "one"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(response["menu"]["paths"], serde_json::json!(["one.txt"]));
}

#[test]
fn editor_menu_uses_focused_file_without_an_argument_or_selection() {
    use openidea_plugin::Plugin;
    let host = metadata_host();
    host.open_editor("/project/one.txt", "text", "sample", &[]);
    let mut plugin = SvnPlugin::default();
    plugin.run_command("log", &host.invocation(), None).unwrap();
    let response = plugin
        .dispatch(
            request("refresh", Some("/project"), "one"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(response["menu"]["paths"], serde_json::json!(["one.txt"]));
}

#[test]
fn workspace_selection_changes_the_real_svn_target_and_isolates_drafts() {
    let host = metadata_host();
    host.add_file(
        ".openidea/workspace.json",
        r#"{"version":1,"projects":["/project","/second"]}"#,
    );
    let mut plugin = SvnPlugin::default();
    let hello = plugin
        .dispatch(request("hello", None, "one"), &host.invocation())
        .unwrap();
    assert_eq!(hello["roots"], serde_json::json!(["/project", "/second"]));
    let req = |action: &str, root: &str, extra: serde_json::Value| {
        let mut value =
            serde_json::json!({"client":"one","generation":2,"root":root,"action":action});
        for (k, v) in extra.as_object().unwrap() {
            value[k] = v.clone();
        }
        Request::parse(&value.to_string()).unwrap()
    };
    plugin
        .dispatch(
            req(
                "draft",
                "/project",
                serde_json::json!({"message":"first draft"}),
            ),
            &host.invocation(),
        )
        .unwrap();
    let switched = plugin
        .dispatch(
            req(
                "select-root",
                "/project",
                serde_json::json!({"directory":"/second"}),
            ),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(switched["root"], "/second");
    assert_eq!(switched["draft"], "");
    plugin
        .dispatch(
            req("refresh", "/second", serde_json::json!({})),
            &host.invocation(),
        )
        .unwrap();
    assert!(
        host.process_runs()
            .iter()
            .any(|r| r.args.last().is_some_and(|a| a == "/second@"))
    );
    assert!(
        plugin
            .dispatch(
                req(
                    "select-root",
                    "/second",
                    serde_json::json!({"directory":"/foreign"})
                ),
                &host.invocation()
            )
            .is_err()
    );
    let back = plugin
        .dispatch(
            req(
                "select-root",
                "/second",
                serde_json::json!({"directory":"/project"}),
            ),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(back["draft"], "first draft");
    plugin
        .dispatch(
            req("update", "/project", serde_json::json!({})),
            &host.invocation(),
        )
        .unwrap();
    assert!(
        plugin
            .dispatch(
                req(
                    "select-root",
                    "/project",
                    serde_json::json!({"directory":"/second"})
                ),
                &host.invocation()
            )
            .is_err()
    );
}

#[test]
fn attached_file_menu_selects_its_own_workspace_directory() {
    use openidea_plugin::Plugin;
    let host = metadata_host();
    host.add_file(
        ".openidea/workspace.json",
        r#"{"projects":["/project","/second"]}"#,
    );
    host.select_paths(&["/second/one.txt"]);
    let mut plugin = SvnPlugin::default();
    plugin.run_command("log", &host.invocation(), None).unwrap();
    let response = plugin
        .dispatch(request("hello", None, "one"), &host.invocation())
        .unwrap();
    assert_eq!(response["menuDirectory"], "/second");
    let switch=Request::parse(r#"{"client":"one","generation":2,"root":"/project","action":"select-root","directory":"/second"}"#).unwrap();
    plugin.dispatch(switch, &host.invocation()).unwrap();
    let response = plugin.dispatch(
        request("refresh", Some("/second"), "one"),
        &host.invocation(),
    );
    // generation must keep increasing after the directory handshake.
    assert!(response.is_err());
    let refresh =
        Request::parse(r#"{"client":"one","generation":3,"root":"/second","action":"refresh"}"#)
            .unwrap();
    let response = plugin.dispatch(refresh, &host.invocation()).unwrap();
    assert_eq!(response["menu"]["paths"], serde_json::json!(["one.txt"]));
}

#[test]
fn pending_menu_directory_is_sent_even_when_current_refresh_fails() {
    use openidea_plugin::{
        Plugin,
        views::{ViewEvent, Views},
    };
    let host = metadata_host();
    host.add_file(
        ".openidea/workspace.json",
        r#"{"projects":["/project","/second"]}"#,
    );
    let mut plugin = SvnPlugin::default();
    plugin
        .dispatch(request("hello", None, "one"), &host.invocation())
        .unwrap();
    host.select_paths(&["/second/a.txt"]);
    plugin.run_command("log", &host.invocation(), None).unwrap();
    host.on_process("svn", |_, _| {
        Ok(openidea_plugin::ProcessOutput {
            status: Some(1),
            stdout: String::new(),
            stderr: "svn: E155007 not a working copy".into(),
        })
    });
    let text =
        serde_json::json!({"client":"one","generation":2,"root":"/project","action":"refresh"})
            .to_string();
    plugin
        .handle("svn", ViewEvent::Message(text), &host.invocation())
        .unwrap();
    let messages = host.view_messages();
    let value: serde_json::Value = serde_json::from_str(&messages.last().unwrap().1).unwrap();
    assert_eq!(value["menuDirectory"], "/second");
}

#[test]
fn windows_mixed_separator_menus_and_dirty_editors_remain_in_scope() {
    use openidea_plugin::Plugin;
    let host = metadata_host();
    host.set_project_root(Some(r"D:\workspace\中文 示例项目"));
    host.select_paths(&["d:/WORKSPACE/中文 示例项目/src/a.js"]);
    let mut plugin = SvnPlugin::default();
    plugin.run_command("log", &host.invocation(), None).unwrap();
    let root = r"D:\workspace\中文 示例项目";
    let response = plugin
        .dispatch(
            request("refresh", Some(root), "windows"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(response["menu"]["paths"], serde_json::json!(["src/a.js"]));
    host.open_editor(
        "d:/workspace/中文 示例项目/src/a.js",
        "javascript",
        "saved",
        &[],
    );
    host.type_in_editor("unsaved");
    let result = plugin.dispatch(request("update", Some(root), "windows"), &host.invocation());
    assert!(result.unwrap_err().to_string().contains("未保存"));
}

#[test]
fn refresh_preferences_default_off_and_persist_for_the_workspace() {
    let host = metadata_host();
    let mut plugin = SvnPlugin::default();
    let hello = plugin
        .dispatch(request("hello", None, "settings-one"), &host.invocation())
        .unwrap();
    assert_eq!(
        hello["settings"],
        serde_json::json!({"enabled":false,"intervalSeconds":30})
    );
    let settings=Request::parse(r#"{"client":"settings-one","generation":2,"root":"/project","action":"settings","settings":{"enabled":true,"intervalSeconds":45}}"#).unwrap();
    let saved = plugin.dispatch(settings, &host.invocation()).unwrap();
    assert_eq!(saved["settings"]["intervalSeconds"], 45);
    let next = plugin
        .dispatch(request("hello", None, "settings-two"), &host.invocation())
        .unwrap();
    assert_eq!(
        next["settings"],
        serde_json::json!({"enabled":true,"intervalSeconds":45})
    );
    host.set_project_root(Some("/unrelated"));
    let other = plugin
        .dispatch(request("hello", None, "settings-other"), &host.invocation())
        .unwrap();
    assert_eq!(other["settings"]["enabled"], false);
    for interval in [0, 4, 3601] {
        let value = serde_json::json!({"client":"bad","generation":1,"root":"/project","action":"settings","settings":{"enabled":true,"intervalSeconds":interval}});
        assert!(Request::parse(&value.to_string()).is_err());
    }
}

#[test]
fn windows_selected_file_and_directory_actions_block_dirty_editors_with_different_case() {
    let host = metadata_host();
    let root = r"D:\repo";
    host.set_project_root(Some(root));
    host.open_editor(r"d:\REPO\SRC\A.js", "javascript", "saved", &[]);
    host.type_in_editor("unsaved");
    let mut plugin = SvnPlugin::default();
    for path in ["src/a.js", "src", "."] {
        let req=Request::parse(&serde_json::json!({"client":"case-check","generation":1,"root":root,"action":"revert","paths":[path]}).to_string()).unwrap();
        let result = plugin.dispatch(req, &host.invocation());
        assert!(
            result.unwrap_err().to_string().contains("未保存"),
            "dirty {path} was not blocked"
        );
    }
}

#[test]
fn change_view_mode_persists_without_changing_refresh_preferences() {
    let host = metadata_host();
    let mut plugin = SvnPlugin::default();
    let hello = plugin
        .dispatch(request("hello", None, "tree-one"), &host.invocation())
        .unwrap();
    assert_eq!(hello["viewMode"], "tree");
    assert_eq!(hello["settings"]["enabled"], false);
    let mode=Request::parse(r#"{"client":"tree-one","generation":2,"root":"/project","action":"view-mode","viewMode":"list"}"#).unwrap();
    let saved = plugin.dispatch(mode, &host.invocation()).unwrap();
    assert_eq!(saved["viewMode"], "list");
    let reopened = plugin
        .dispatch(request("hello", None, "tree-two"), &host.invocation())
        .unwrap();
    assert_eq!(reopened["viewMode"], "list");
    assert_eq!(reopened["settings"]["enabled"], false);
    assert!(Request::parse(r#"{"client":"tree-one","generation":3,"root":"/project","action":"view-mode","viewMode":"arbitrary"}"#).is_err());
}

#[test]
fn ignored_entries_are_requested_explicitly_and_ignore_cancellation_does_not_write() {
    let host = MockHost::install().with_capabilities(&[
        "project.read",
        "editor.read",
        "storage",
        "process:svn",
    ]);
    host.add_file("cache.tmp", "local");
    host.on_process("svn",|args,_|{
        let text=match args[0].as_str(){
            "status" if args.iter().any(|a|a=="--no-ignore")=>"<status><target path=\".\"><entry path=\"cache.tmp\"><wc-status item=\"ignored\" props=\"none\"/></entry></target></status>",
            "status"=>"<status><target path=\".\"><entry path=\"cache.tmp\"><wc-status item=\"unversioned\" props=\"none\"/></entry></target></status>",
            "proplist"=>"<properties/>",
            _=>"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><wc-info><wcroot-abspath>/project</wcroot-abspath></wc-info></entry></info>"
        };Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout:text.into(),stderr:String::new()})
    });
    let req = |action: &str, extra: serde_json::Value| {
        let mut value = serde_json::json!({"client":"ignore-one","root":"/project","generation":1,"action":action});
        for (k, v) in extra.as_object().unwrap() {
            value[k] = v.clone();
        }
        Request::parse(&value.to_string()).unwrap()
    };
    let mut plugin = SvnPlugin::default();
    let shown = plugin
        .dispatch(
            req("refresh", serde_json::json!({"includeIgnored":true})),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(shown["entries"][0]["status"], "ignored");
    let hidden = plugin
        .dispatch(
            req("refresh", serde_json::json!({"includeIgnored":false})),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(hidden["entries"][0]["status"], "unversioned");
    host.answer_next_prompt(None);
    let cancelled = plugin
        .dispatch(
            req("ignore", serde_json::json!({"paths":["cache.tmp"]})),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(cancelled["busy"], false);
    assert_eq!(cancelled["status"], "已取消");
    assert!(
        host.process_runs()
            .iter()
            .all(|r| !matches!(r.args[0].as_str(), "propset" | "propdel"))
    );
}

#[test]
fn directory_revert_is_explicit_confirmed_and_rejects_unknown_folders() {
    let host = metadata_host();
    host.on_process("svn",|args,_|{
        let xml=if args[0]=="status" {"<status><target path=\".\"><entry path=\"src/a.txt\"><wc-status item=\"modified\" props=\"none\"/></entry><entry path=\"src/keep.tmp\"><wc-status item=\"unversioned\" props=\"none\"/></entry></target></status>"}else{"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><wc-info><wcroot-abspath>/project</wcroot-abspath></wc-info></entry></info>"};
        Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout:xml.into(),stderr:String::new()})
    });
    let req = |directory: &str| {
        Request::parse(&serde_json::json!({"client":"folder-revert","generation":1,"root":"/project","action":"revert","directory":directory,"paths":[directory]}).to_string()).unwrap()
    };
    let mut plugin = SvnPlugin::default();
    host.answer_next_prompt(None);
    let cancelled = plugin.dispatch(req("src"), &host.invocation()).unwrap();
    assert_eq!(cancelled["status"], "已取消");
    host.answer_next_prompt(Some(&["yes"]));
    let result = plugin.dispatch(req("src"), &host.invocation()).unwrap();
    assert_eq!(result["busy"], true);
    plugin
        .dispatch(
            request("cancel", Some("/project"), "folder-revert"),
            &host.invocation(),
        )
        .unwrap();
    assert!(
        plugin
            .dispatch(req("unrelated"), &host.invocation())
            .is_err()
    );
    host.open_editor("/project/src/a.txt", "text", "base", &[]);
    host.type_in_editor("unsaved");
    assert!(
        plugin
            .dispatch(req("src"), &host.invocation())
            .unwrap_err()
            .to_string()
            .contains("未保存")
    );
}

#[test]
fn root_ignore_property_change_remains_committable() {
    let host = metadata_host();
    host.add_file(".svn/entries", "metadata");
    host.add_dir("./.svn");
    host.on_process("svn",|args,_|{
        let xml=if args[0]=="status"{"<status><target path=\".\"><entry path=\".\"><wc-status item=\"normal\" props=\"modified\"/></entry></target></status>"}else{"<info><entry path=\".\" kind=\"dir\" revision=\"1\"><url>file:///repo</url><wc-info><wcroot-abspath>/project</wcroot-abspath></wc-info></entry></info>"};Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout:xml.into(),stderr:String::new()})
    });
    let mut plugin = SvnPlugin::default();
    let response = plugin
        .dispatch(
            request("refresh", Some("/project"), "root-rules"),
            &host.invocation(),
        )
        .unwrap();
    assert_eq!(response["entries"][0]["status"], "normal");
    let entries: Vec<openidea_svn::svn::Entry> =
        serde_json::from_value(response["entries"].clone()).unwrap();
    openidea_svn::svn::validate_commit(&entries, &[".".into()]).unwrap();
}
