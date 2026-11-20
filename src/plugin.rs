use crate::{
    preferences::{ChangeView, RefreshSettings},
    protocol::Request,
    svn::{self, Entry, Info, LogEntry},
    working_copy as wc,
    workspace::{self, Scope},
};
use openidea_plugin::{
    Context, Error, Event, EventKind, Invocation, Plugin, Task, host,
    views::{View, ViewEvent, Viewport, Views},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const VIEW: &str = "svn";
const HISTORY_LIMIT: usize = 2000;
struct Job {
    child: host::Child,
    task: Task,
    action: String,
    started: Instant,
    stderr: String,
}
#[derive(Default)]
struct Session {
    root: String,
    window_root: String,
    generation: u64,
    info: Option<Info>,
    entries: Vec<Entry>,
    include_ignored: bool,
    logs: Vec<LogEntry>,
    job: Option<Job>,
    draft: String,
    pending_output: String,
    completed: Option<(i32, String, String)>,
}
#[derive(Default)]
pub struct SvnPlugin {
    sessions: HashMap<String, Session>,
    pending: HashMap<String, Value>,
}

impl SvnPlugin {
    pub fn dispatch(&mut self, request: Request, ctx: &Invocation) -> Result<Value, Error> {
        let project = ctx.project().ok_or("请先打开一个 SVN 项目。")?;
        let window_root = project.root()?;
        let roots = workspace::roots(&project)?;
        self.reap_jobs();
        let existing = self.sessions.get(&request.client);
        if existing.is_some_and(|s| s.window_root != window_root) {
            return Err(wc::error("此面板已绑定其他窗口。"));
        }
        let current = existing.map(|s| s.root.clone()).unwrap_or_else(|| {
            host::storage::get(&format!("selected:{window_root}"))
                .ok()
                .flatten()
                .filter(|r| roots.contains(r))
                .unwrap_or_else(|| roots[0].clone())
        });
        request.check_project(&current)?;
        let root = if request.action == "select-root" {
            if existing.is_some_and(|s| s.job.is_some()) {
                return Err(wc::error("请等待当前操作完成后切换目录。"));
            }
            let directory = request.directory.as_ref().ok_or("请选择工作目录。")?;
            if !roots.contains(directory) {
                return Err(wc::error("只能选择当前工作区中的目录。"));
            }
            if existing.is_some_and(|s| request.generation < s.generation) {
                return Err(Error::Stale);
            }
            directory.clone()
        } else {
            current
        };
        if !roots.contains(&root)
            && !matches!(
                request.action.as_str(),
                "hello" | "select-root" | "view-mode" | "settings" | "poll" | "cancel" | "draft"
            )
        {
            return Err(wc::error("所选目录已从工作区移除，请选择其他目录。"));
        }
        let project = Scope::new(project, root.clone());
        let lock_info = self
            .sessions
            .get(&request.client)
            .and_then(|s| s.info.as_ref())
            .filter(|info| !info.wc_root.is_empty());
        let lock_root = lock_info.map(|info| info.wc_root.as_str()).unwrap_or(&root);
        if !matches!(
            request.action.as_str(),
            "hello" | "select-root" | "view-mode" | "settings" | "poll" | "cancel" | "draft"
        ) && self.sessions.iter().any(|(id, s)| {
            id != &request.client
                && s.job.is_some()
                && (s.root == root
                    || s.info.as_ref().is_some_and(|i| {
                        !i.wc_root.is_empty()
                            && (workspace::relative_path(&i.wc_root, lock_root).as_deref()
                                == Some(".")
                                || lock_info.is_none()
                                    && workspace::relative_path(&i.wc_root, &root).is_some())
                    }))
        }) {
            return Err(wc::error(
                "此工作副本正在另一个 SVN 面板中执行操作，请等待完成。",
            ));
        }
        if !self.sessions.contains_key(&request.client) && self.sessions.len() >= 32 {
            self.sessions.retain(|_, s| s.job.is_some());
            if self.sessions.len() >= 32 {
                return Err(wc::error("打开的 SVN 会话过多，请关闭不用的项目。"));
            }
        }
        if request.action == "select-root" {
            host::storage::set(&format!("selected:{window_root}"), &root)?;
            self.sessions.remove(&request.client);
        }
        let mut session = self
            .sessions
            .remove(&request.client)
            .unwrap_or_else(|| Session {
                root: root.clone(),
                window_root: window_root.clone(),
                draft: host::storage::get(&format!("draft:{root}:{}", request.client))
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
                ..Default::default()
            });
        let mut result = self.handle_request(&mut session, &request, &project);
        if let Ok(response) = &mut result {
            let pending_output = std::mem::take(&mut session.pending_output);
            if !pending_output.is_empty() {
                response["output"] = json!(format!(
                    "{pending_output}{}",
                    response["output"].as_str().unwrap_or("")
                ));
            }
            if let Some((status, stderr, action)) = session.completed.take() {
                if session.job.is_none() && request.action != "refresh" {
                    response["needsRefresh"] = json!(true);
                }
                if status == 0 {
                    if action == "commit" && session.job.is_none() {
                        response["clearMessage"] = json!(true);
                    }
                } else {
                    response["error"] = json!(if stderr.is_empty() {
                        format!("SVN 退出码 {status}，请查看操作输出。")
                    } else {
                        wc::svn_error(&stderr).to_string()
                    });
                }
            }
        }
        self.sessions.insert(request.client.clone(), session);
        let mut response = result?;
        response["roots"] = json!(roots);
        response["client"] = json!(request.client);
        response["root"] = json!(root);
        response["generation"] = json!(request.generation);
        if let Some(menu) = self.pending.get(&window_root) {
            if menu["directory"] != root {
                response["menuDirectory"] = menu["directory"].clone();
            } else if request.action == "refresh" {
                response["menu"] = self.pending.remove(&window_root).unwrap();
            }
        }
        Ok(response)
    }

    /// Any panel can reap a finished owner-bound process. A closed window cannot
    /// leave a permanent lock merely because its iframe stopped sending polls.
    fn reap_jobs(&mut self) {
        for (client, session) in &mut self.sessions {
            let Some(job) = session.job.as_mut() else {
                continue;
            };
            let mut finished = None;
            if job.task.is_cancelled() || job.started.elapsed() > Duration::from_secs(1800) {
                let _ = job.child.kill();
                tail(&mut job.stderr, "操作已取消或超时。", 65536);
            }
            job.task
                .report(0, None, Some(&format!("SVN {}…", job.action)));
            // Once status is known, drain the remaining bounded stream before
            // dropping the child. A single poll may report exit with bytes queued.
            for index in 0..128 {
                match job.child.poll(65536) {
                    Ok(chunk) => {
                        let empty = chunk.stdout.is_empty() && chunk.stderr.is_empty();
                        tail(&mut session.pending_output, &chunk.stdout, 1024 * 1024);
                        tail(&mut session.pending_output, &chunk.stderr, 1024 * 1024);
                        tail(&mut job.stderr, &chunk.stderr, 65536);
                        if let Some(status) = chunk.status {
                            if empty || index == 127 {
                                if index == 127 {
                                    tail(
                                        &mut session.pending_output,
                                        "\n[输出超过读取限制，剩余内容已截断]\n",
                                        1024 * 1024,
                                    );
                                }
                                finished = Some((status, job.stderr.clone(), job.action.clone()));
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    Err(error) => {
                        finished = Some((
                            -1,
                            format!("原进程已终止或不可访问：{error}"),
                            job.action.clone(),
                        ));
                        break;
                    }
                }
            }
            if let Some(completion) = finished {
                if completion.0 == 0 && completion.2 == "commit" {
                    session.draft.clear();
                    let _ = host::storage::delete(&format!("draft:{}:{client}", session.root));
                }
                session.completed = Some(completion);
                session.job.take();
            }
        }
    }

    fn handle_request(&mut self, s: &mut Session, r: &Request, p: &Scope) -> Result<Value, Error> {
        if r.generation < s.generation {
            return Err(Error::Stale);
        }
        s.generation = r.generation;
        if s.job.is_some()
            && !matches!(
                r.action.as_str(),
                "poll" | "cancel" | "draft" | "hello" | "view-mode" | "settings"
            )
        {
            return Err(wc::error("请等待当前操作完成。"));
        }
        let mut result = json!({"busy":s.job.is_some()});
        match r.action.as_str() {
            "hello" | "select-root" => {
                result["type"] = json!("hello");
                result["draft"] = json!(s.draft);
                result["settings"] = json!(RefreshSettings::load(&s.window_root));
                result["viewMode"] = json!(ChangeView::load(&s.window_root));
            }
            "view-mode" => {
                let mode = r.view_mode.ok_or("请选择列表或树状视图。")?;
                mode.save(&s.window_root)?;
                result["viewMode"] = json!(mode);
            }
            "settings" => {
                let settings = r.settings.as_ref().ok_or("缺少刷新配置。")?;
                settings.save(&s.window_root)?;
                result["settings"] = json!(settings);
            }
            "draft" => {
                s.draft = r.message.clone();
                host::storage::set(&format!("draft:{}:{}", s.root, r.client), &s.draft)?;
            }
            "refresh" => {
                let include_ignored = r.include_ignored.unwrap_or(s.include_ignored);
                let (info, entries) = wc::refresh_with_ignored(p, include_ignored)?;
                s.include_ignored = include_ignored;
                s.info = Some(info);
                s.entries = entries;
                result["info"] = json!(s.info);
                result["entries"] = json!(s.entries);
                result["includeIgnored"] = json!(include_ignored);
            }
            "authenticate" => {
                host::open_terminal(
                    "SVN 认证",
                    "svn",
                    &[
                        "info".into(),
                        "--revision".into(),
                        "HEAD".into(),
                        "--".into(),
                        format!("{}@", s.root),
                    ],
                )?;
                result["status"] = json!("在 IDE 终端完成认证后，点击刷新或重试。");
            }
            "log" => {
                if s.info.is_none() {
                    let (info, entries) = wc::refresh(p)?;
                    s.info = Some(info);
                    s.entries = entries;
                }
                let mut logs = svn::parse_log(&wc::read(
                    p,
                    &svn::command("log", &r.paths, "", r.revision)?,
                )?)?;
                let full_page = logs.len() == 50 && logs.last().is_some_and(|log| log.revision > 1);
                if !r.append {
                    s.logs.clear();
                }
                logs.truncate(HISTORY_LIMIT.saturating_sub(s.logs.len()));
                result["logs"] = json!(logs);
                result["append"] = json!(r.append);
                s.logs.extend(logs);
                result["hasMore"] = json!(full_page && s.logs.len() < HISTORY_LIMIT);
                result["historyLimitReached"] = json!(s.logs.len() >= HISTORY_LIMIT);
            }
            "diff" => {
                let path = r.paths.first().ok_or("请选择文件。")?;
                wc::local_diff(p, path, &s.entries)?;
            }
            "history-diff" => {
                wc::history_diff(
                    p,
                    s.info.as_ref().ok_or("请先刷新工作副本。")?,
                    &s.logs,
                    r.revision.ok_or("请选择版本。")?,
                    r.path.as_deref().ok_or("请选择历史文件。")?,
                )?;
            }
            "open" => {
                for path in &r.paths {
                    svn::target(path)?;
                    host::open_editor(&format!("{}/{}", s.root.trim_end_matches('/'), path), None).map_err(|error| {
                        if !p.direct { wc::error(format!("当前 OpenIDEA 版本限制插件直接打开附加目录文件，请从左侧项目树打开。差异和 SVN 操作仍可在此面板完成。\n{error}")) } else { error }
                    })?;
                }
            }
            "cancel" => {
                if let Some(job) = s.job.take() {
                    job.child.kill()?;
                    result["output"] = json!("\n操作已取消；已发生的 SVN 更改仍需检查。\n");
                    result["needsRefresh"] = json!(true);
                }
                result["busy"] = json!(false);
            }
            "poll" => {}
            action => {
                let all = action == "update";
                let ignore = matches!(action, "ignore" | "unignore");
                if !ignore {
                    wc::dirty(p, &r.paths, all)?;
                }
                let mut paths = r.paths.clone();
                paths.sort();
                paths.dedup();
                let directory_revert = action == "revert" && r.directory.is_some();
                if matches!(action, "commit" | "revert") {
                    let (info, entries) = wc::refresh(p)?;
                    s.info = Some(info);
                    s.entries = entries;
                    if action == "commit" {
                        svn::validate_commit(&s.entries, &paths)?;
                    }
                }
                if directory_revert {
                    wc::check_revert_directory(p, &paths[0])?;
                }
                if matches!(action, "add" | "delete" | "revert" | "resolve") && !directory_revert {
                    if paths.is_empty() {
                        return Err(wc::error("请选择操作目标。"));
                    }
                    for path in &paths {
                        let entry = s
                            .entries
                            .iter()
                            .find(|e| e.path == *path)
                            .ok_or("所选路径不在当前变更列表，请刷新。")?;
                        if entry.status == "external" {
                            return Err(wc::error("请将外部工作副本单独作为项目打开。"));
                        }
                        if action == "add" && entry.status != "unversioned" {
                            return Err(wc::error("仅能添加未跟踪文件。"));
                        }
                        if action == "revert"
                            && matches!(entry.status.as_str(), "unversioned" | "ignored")
                        {
                            return Err(wc::error("未跟踪文件不会通过还原操作删除。"));
                        }
                    }
                }
                if action == "revert" && !directory_revert {
                    paths = wc::selected_revert_paths(p, &s.entries, &paths)?;
                }
                let command = if directory_revert {
                    "revert-tree"
                } else if action == "resolve" {
                    match r.accept.as_deref() {
                        Some("mine-full") => "resolve-mine",
                        Some("theirs-full") => "resolve-theirs",
                        Some("working") => "resolve-working",
                        _ => return Err(wc::error("请选择明确的冲突解决方式。")),
                    }
                } else {
                    action
                };
                let args = if ignore {
                    let Some(args) =
                        wc::prepare_ignore(p, &paths, action == "unignore", &r.client)?
                    else {
                        result["status"] = json!("已取消");
                        return Ok(result);
                    };
                    p.args(&args)
                } else {
                    p.args(&svn::command(command, &paths, &r.message, None)?)
                };
                if matches!(action, "commit" | "delete" | "revert" | "resolve") {
                    let body = format!(
                        "目录：{}\n操作：{action}\n\n{}\n\n{}",
                        s.root,
                        paths.join("\n"),
                        if directory_revert {
                            "将还原此目录下的全部已跟踪修改。新增路径会取消版本控制；未跟踪和已忽略文件不会被删除。"
                        } else if action == "commit" {
                            r.message.as_str()
                        } else {
                            "此操作可能丢弃所选文件的本地修改。目录删除包括所有后代。"
                        }
                    );
                    if !host::confirm(
                        if action == "commit" {
                            "确认提交所选路径"
                        } else {
                            "确认 SVN 操作"
                        },
                        &body,
                    )? {
                        result["status"] = json!("已取消");
                        return Ok(result);
                    }
                    wc::dirty(p, &paths, false)?;
                    if directory_revert {
                        wc::check_revert_directory(p, &paths[0])?;
                    } else if action == "revert" {
                        let (_, latest) = wc::refresh(p)?;
                        if wc::selected_revert_paths(p, &latest, &paths)? != paths {
                            return Err(wc::error("所选路径状态已变化，请刷新后重新确认。"));
                        }
                    }
                }
                let task = host::begin_task(&format!("SVN {action}"), true);
                task.report(0, None, Some("正在启动…"));
                let child =
                    wc::with_utf8_locale(&args, |command| host::start(command, Some(&p.project)))?;
                child.close_stdin()?;
                s.job = Some(Job {
                    child,
                    task,
                    action: action.into(),
                    started: Instant::now(),
                    stderr: String::new(),
                });
                result["busy"] = json!(true);
                result["output"] = json!(format!("\n[SVN {action}] {}\n", paths.join(", ")));
                result["status"] = json!(format!("正在执行 {action}…"));
            }
        }
        Ok(result)
    }
}

fn tail(buffer: &mut String, text: &str, max: usize) {
    buffer.push_str(text);
    if buffer.len() > max {
        let mut cut = buffer.len() - max;
        while !buffer.is_char_boundary(cut) {
            cut += 1;
        }
        buffer.drain(..cut);
        buffer.insert_str(0, "[输出已截断，仅保留末尾内容]\n");
    }
}

impl Plugin for SvnPlugin {
    fn activate(&mut self, _: &Context) -> Result<(), Error> {
        host::subscribe(EventKind::FileSaved)?;
        host::subscribe(EventKind::FilesChanged)?;
        Ok(())
    }
    fn run_command(
        &mut self,
        id: &str,
        ctx: &Invocation,
        argument: Option<&str>,
    ) -> Result<Option<String>, Error> {
        let p = ctx.project().ok_or("请先打开一个项目。")?;
        let window_root = p.root()?;
        let roots = workspace::roots(&p)?;
        let mut selected = ctx.selected_paths();
        // The editor's context menu passes its bare path as the command
        // argument; project/tab menus can additionally fill selected_paths.
        if selected.is_empty()
            && let Some(argument) = argument
        {
            let path = if argument.starts_with('{') {
                serde_json::from_str::<Value>(argument)
                    .ok()
                    .and_then(|v| v["path"].as_str().map(str::to_owned))
                    .ok_or("无效的文件菜单参数。")?
            } else {
                argument.to_owned()
            };
            selected.push(path);
        }
        // OpenIDEA editor.context intentionally supplies neither argument nor
        // paths: its target is the invocation's focused text editor.
        if selected.is_empty()
            && matches!(id, "diff" | "commit" | "log" | "add" | "revert")
            && let Some(editor) = ctx.editor()
        {
            selected.push(editor.path());
        }
        let root = if let Some(first) = selected.first() {
            roots
                .iter()
                .filter(|root| workspace::relative_path(root, first).is_some())
                .max_by_key(|root| root.len())
                .cloned()
                .ok_or("所选文件不在当前工作区。")?
        } else {
            host::storage::get(&format!("selected:{window_root}"))?
                .filter(|r| roots.contains(r))
                .unwrap_or_else(|| roots[0].clone())
        };
        let mut paths = Vec::new();
        for path in selected {
            let relative = workspace::relative_path(&root, &path)
                .ok_or("所选文件不在当前项目内，请单独打开其工作副本。")?;
            svn::target(&relative)?;
            paths.push(relative);
        }
        self.pending.insert(
            window_root,
            json!({"action":id,"paths":paths,"directory":root}),
        );
        host::reveal_view(VIEW)?;
        let _ = host::post_view_message(VIEW, &json!({"type":"menu-pending"}).to_string());
        Ok(None)
    }
    fn handle_events(&mut self, _: &[Event], ctx: &Invocation) {
        self.reap_jobs();
        let Some(p) = ctx.project() else { return };
        let Ok(root) = p.root() else { return };
        for (client, s) in &self.sessions {
            if s.window_root == root {
                let _ = host::post_view_message(
                    VIEW,
                    &json!({"type":"invalidate","client":client}).to_string(),
                );
            }
        }
    }
}
impl Views for SvnPlugin {
    fn render(&mut self, _: &str, _: Option<Viewport>, _: &Invocation) -> Result<View, Error> {
        Ok(View::message("SVN"))
    }
    fn handle(&mut self, _: &str, event: ViewEvent, ctx: &Invocation) -> Result<(), Error> {
        if let ViewEvent::Message(text) = event {
            let request = match Request::parse(&text) {
                Ok(r) => r,
                Err(e) => {
                    host::notify(openidea_plugin::NotificationLevel::Error, "SVN", &e);
                    return Ok(());
                }
            };
            let mut result = match self.dispatch(request.clone(), ctx) {
                Ok(value) => value,
                Err(error) => {
                    json!({"client":request.client,"generation":request.generation,"root":request.root.clone().or_else(||ctx.project().and_then(|p|p.root().ok())),"busy":self.sessions.get(&request.client).is_some_and(|s|s.job.is_some()),"error":error.to_string()})
                }
            };
            if let Some(menu) = ctx
                .project()
                .and_then(|p| p.root().ok())
                .and_then(|root| self.pending.get(&root))
                && menu["directory"] != result["root"]
            {
                result["menuDirectory"] = menu["directory"].clone();
            }
            host::post_view_message(VIEW, &result.to_string())?;
        }
        Ok(())
    }
}
openidea_plugin::register_plugin!(SvnPlugin, views);

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn req(action: &str, client: &str) -> Request {
        Request::parse(
            &json!({"client":client,"generation":1,"root":"/project","action":action}).to_string(),
        )
        .unwrap()
    }
    #[test]
    fn a_new_panel_reaps_a_process_ended_by_its_old_window() {
        let host = MockHost::install().with_capabilities(&[
            "project.read",
            "editor.read",
            "storage",
            "process:svn",
        ]);
        let mut plugin = SvnPlugin::default();
        plugin
            .dispatch(req("update", "old"), &host.invocation())
            .unwrap();
        plugin.sessions["old"]
            .job
            .as_ref()
            .unwrap()
            .child
            .kill()
            .unwrap();
        let result = plugin.dispatch(req("cleanup", "new"), &host.invocation());
        assert!(
            result.is_ok(),
            "closed-window job retained its write lock: {result:?}"
        );
    }
    #[test]
    fn process_completion_drains_output_beyond_the_first_poll_chunk() {
        let host = MockHost::install().with_capabilities(&[
            "project.read",
            "editor.read",
            "storage",
            "process:svn",
        ]);
        let mut plugin = SvnPlugin::default();
        plugin
            .dispatch(req("update", "one"), &host.invocation())
            .unwrap();
        let project = host.invocation().project().unwrap();
        let child = host::start(&host::Command::new("svn"), Some(&project)).unwrap();
        child
            .write(&format!("{}FINAL_DIAGNOSTIC", "a".repeat(150000)))
            .unwrap();
        child.kill().unwrap();
        plugin
            .sessions
            .get_mut("one")
            .unwrap()
            .job
            .as_mut()
            .unwrap()
            .child = child;
        let result = plugin
            .dispatch(req("poll", "one"), &host.invocation())
            .unwrap();
        assert!(
            result["output"]
                .as_str()
                .unwrap()
                .contains("FINAL_DIAGNOSTIC")
        );
        assert_eq!(result["busy"], false);
    }

    #[test]
    fn history_pages_stop_at_the_retention_limit_without_exposing_unreadable_rows() {
        let host = MockHost::install().with_capabilities(&["project.read", "process:svn"]);
        host.on_process("svn",|args,_| {
            let stdout=if args[0]=="log" {format!("<log>{}</log>",(961..=1010).rev().map(|revision|format!("<logentry revision=\"{revision}\"><paths><path action=\"M\" kind=\"file\">/f.txt</path></paths><msg>m</msg></logentry>")).collect::<String>())}else{"content".into()};
            Ok(openidea_plugin::ProcessOutput{status:Some(0),stdout,stderr:String::new()})
        });
        let mut plugin = SvnPlugin::default();
        plugin.sessions.insert(
            "one".into(),
            Session {
                root: "/project".into(),
                window_root: "/project".into(),
                info: Some(Info {
                    root_url: "file:///repo".into(),
                    ..Default::default()
                }),
                logs: (1011..=3000)
                    .rev()
                    .map(|revision| LogEntry {
                        revision,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        );
        let request=Request::parse(&json!({"client":"one","generation":1,"root":"/project","action":"log","append":true,"revision":1010}).to_string()).unwrap();
        let result = plugin.dispatch(request, &host.invocation()).unwrap();
        assert_eq!(
            result["logs"].as_array().unwrap().len(),
            10,
            "host exposed rows beyond its retained history"
        );
        assert_eq!(result["hasMore"], false);
        assert_eq!(result["historyLimitReached"], true);
        assert_eq!(plugin.sessions["one"].logs.len(), 2000);
        let request=Request::parse(&json!({"client":"one","generation":2,"root":"/project","action":"history-diff","revision":1001,"path":"/f.txt"}).to_string()).unwrap();
        plugin.dispatch(request, &host.invocation()).unwrap();
        assert_eq!(host.opened_diffs().len(), 1);
    }
}
