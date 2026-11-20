use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub status: String,
    pub props: String,
    pub tree_conflicted: bool,
    pub kind: String,
    #[serde(default)]
    pub metadata_complete: bool,
    #[serde(default)]
    pub copied: bool,
    #[serde(default)]
    pub replaced: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub url: String,
    pub root_url: String,
    pub wc_root: String,
    pub revision: u64,
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub revision: u64,
    pub author: String,
    pub date: String,
    pub message: String,
    pub paths: Vec<LogPath>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPath {
    pub path: String,
    pub action: String,
    pub kind: String,
    pub copy_from_path: Option<String>,
    pub copy_from_revision: Option<u64>,
}

/// A literal project-relative target. The trailing @ disables SVN peg parsing.
pub fn target(path: &str) -> Result<String, String> {
    checked_target(path, false)
}
fn management_path(path: &str) -> bool {
    path.split('/').any(|p| matches!(p, ".svn" | ".git"))
}
fn checked_target(path: &str, allow_management: bool) -> Result<String, String> {
    if path.is_empty()
        || path.len() > 4000
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
        || path.split('/').any(|p| p == ".." || p.is_empty())
        || (!allow_management && management_path(path))
    {
        return Err("目标必须是项目内的相对路径，且不能包含控制字符或版本控制管理目录。".into());
    }
    Ok(format!("{path}@"))
}

#[derive(Deserialize)]
struct StatusXml {
    #[serde(default, rename = "target")]
    targets: Vec<StatusTarget>,
    #[serde(default, rename = "changelist")]
    changelists: Vec<StatusTarget>,
}
#[derive(Deserialize)]
struct StatusTarget {
    #[serde(default, rename = "entry")]
    entries: Vec<StatusEntry>,
}
#[derive(Deserialize)]
struct StatusEntry {
    #[serde(rename = "@path")]
    path: String,
    #[serde(rename = "wc-status")]
    wc: WcStatus,
}
#[derive(Deserialize)]
struct WcStatus {
    #[serde(rename = "@item")]
    item: String,
    #[serde(default, rename = "@props")]
    props: String,
    #[serde(default, rename = "@tree-conflicted")]
    tree_conflicted: String,
    #[serde(default, rename = "@file-external")]
    file_external: String,
    #[serde(default, rename = "@copied")]
    copied: String,
}
pub(crate) fn parse_xml<T: serde::de::DeserializeOwned>(
    xml: &str,
    root: &str,
) -> Result<T, String> {
    if xml.len() > 8 * 1024 * 1024 {
        return Err("SVN XML 超过 8 MiB，请缩小操作范围。".into());
    }
    let text = xml.trim_start();
    let text = if text.starts_with("<?xml") {
        text.split_once("?>")
            .ok_or("SVN XML 声明不完整")?
            .1
            .trim_start()
    } else {
        text
    };
    if !text.starts_with(&format!("<{root}")) {
        return Err("SVN 未返回预期的 XML 文档。".into());
    }
    quick_xml::de::from_str(xml).map_err(|e| format!("无法解析 SVN XML：{e}"))
}
pub fn parse_status(xml: &str) -> Result<Vec<Entry>, String> {
    parse_status_at(xml, None)
}
pub fn parse_status_at(xml: &str, root: Option<&str>) -> Result<Vec<Entry>, String> {
    let parsed: StatusXml = parse_xml(xml, "status")?;
    let mut entries = Vec::new();
    for entry in parsed
        .targets
        .into_iter()
        .chain(parsed.changelists)
        .flat_map(|t| t.entries)
    {
        let untracked = matches!(entry.wc.item.as_str(), "unversioned" | "ignored");
        let path = relative_xml_path(&entry.path, root, untracked)?;
        if untracked && management_path(&path) {
            continue;
        }
        if entry.wc.item == "normal"
            && entry.wc.props != "modified"
            && entry.wc.tree_conflicted != "true"
        {
            continue;
        }
        let replaced = entry.wc.item == "replaced";
        entries.push(Entry {
            path,
            status: if entry.wc.file_external == "true" {
                "external".into()
            } else {
                entry.wc.item
            },
            props: entry.wc.props,
            tree_conflicted: entry.wc.tree_conflicted == "true",
            kind: "unknown".into(),
            metadata_complete: false,
            copied: entry.wc.copied == "true",
            replaced,
        });
    }
    Ok(entries)
}
#[derive(Deserialize)]
struct InfoXml {
    #[serde(default, rename = "entry")]
    entries: Vec<InfoEntry>,
}
#[derive(Deserialize)]
struct InfoEntry {
    #[serde(default, rename = "@path")]
    path: String,
    #[serde(default, rename = "@kind")]
    kind: String,
    #[serde(default, rename = "@revision")]
    revision: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    repository: RepositoryXml,
    #[serde(default, rename = "wc-info")]
    wc: WcInfoXml,
}
#[derive(Default, Deserialize)]
struct RepositoryXml {
    #[serde(default)]
    root: String,
}
#[derive(Default, Deserialize)]
struct WcInfoXml {
    #[serde(default, rename = "wcroot-abspath")]
    wc_root: String,
    #[serde(default)]
    schedule: String,
}
pub fn parse_info(xml: &str) -> Result<Info, String> {
    let parsed: InfoXml = parse_xml(xml, "info")?;
    let first = parsed
        .entries
        .into_iter()
        .next()
        .ok_or("此目录不是有效的 SVN 工作副本。")?;
    Ok(Info {
        url: first.url,
        root_url: first.repository.root,
        wc_root: first.wc.wc_root,
        revision: first.revision.parse().unwrap_or(0),
        kind: first.kind,
    })
}

pub fn enrich_info(entries: &mut Vec<Entry>, xml: &str) -> Result<(), String> {
    enrich_info_at(entries, xml, None)
}
pub(crate) fn info_paths(xml: &str, root: &str) -> Result<Vec<String>, String> {
    let info: InfoXml = parse_xml(xml, "info")?;
    info.entries
        .into_iter()
        .map(|node| relative_xml_path(&node.path, Some(root), false))
        .collect()
}

pub fn order_revert(entries: &[Entry], selected: &[String]) -> Result<Vec<String>, String> {
    let mut paths = selected.to_vec();
    for path in &paths {
        let entry = entries
            .iter()
            .find(|e| e.path == *path)
            .ok_or("所选路径已变化，请刷新。")?;
        validate_revert_node(entry)?;
        if matches!(
            entry.status.as_str(),
            "unversioned" | "ignored" | "external"
        ) {
            return Err("不能还原未跟踪、已忽略或外部副本项目。".into());
        }
    }
    let added: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.status == "added"
                && e.kind != "file"
                && e.kind != "symlink"
                && paths.contains(&e.path)
        })
        .map(|e| e.path.as_str())
        .collect();
    let in_added = |path: &str| {
        added
            .iter()
            .any(|parent| path == *parent || path.starts_with(&format!("{parent}/")))
    };
    let depth = |path: &str| path.split('/').count();
    paths.sort_by(|a, b| {
        in_added(b)
            .cmp(&in_added(a))
            .then_with(|| {
                if in_added(a) {
                    depth(b).cmp(&depth(a))
                } else {
                    depth(a).cmp(&depth(b))
                }
            })
            .then_with(|| a.cmp(b))
    });
    paths.dedup();
    Ok(paths)
}
pub fn enrich_info_at(
    entries: &mut Vec<Entry>,
    xml: &str,
    root: Option<&str>,
) -> Result<(), String> {
    let info: InfoXml = parse_xml(xml, "info")?;
    for node in info.entries {
        let path = relative_xml_path(&node.path, root, false)?;
        if let Some(entry) = entries.iter_mut().find(|e| e.path == path) {
            entry.kind = node.kind;
            entry.metadata_complete = true;
        } else if node.wc.schedule == "delete" {
            entries.push(Entry {
                path,
                status: "deleted".into(),
                kind: node.kind,
                metadata_complete: true,
                ..Default::default()
            });
        }
    }
    Ok(())
}
pub fn text_content(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("文件超过 1 MiB，无法在插件差异视图中显示。".into());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "此文件不是 UTF-8 文本，无法显示文本差异。")?;
    if text.contains('\0') || text.contains('\u{fffd}') {
        return Err("此文件包含二进制或无法解码的内容，无法显示文本差异。".into());
    }
    Ok(text.to_owned())
}

#[derive(Deserialize)]
struct LogXml {
    #[serde(default, rename = "logentry")]
    entries: Vec<LogEntryXml>,
}
#[derive(Deserialize)]
struct LogEntryXml {
    #[serde(rename = "@revision")]
    revision: u64,
    #[serde(default)]
    author: String,
    #[serde(default)]
    date: String,
    #[serde(default, rename = "msg")]
    message: String,
    #[serde(default)]
    paths: LogPathsXml,
}
#[derive(Default, Deserialize)]
struct LogPathsXml {
    #[serde(default, rename = "path")]
    paths: Vec<LogPathXml>,
}
#[derive(Deserialize)]
struct LogPathXml {
    #[serde(rename = "$text", default)]
    path: String,
    #[serde(rename = "@action", default)]
    action: String,
    #[serde(rename = "@kind", default)]
    kind: String,
    #[serde(rename = "@copyfrom-path")]
    copy_from_path: Option<String>,
    #[serde(rename = "@copyfrom-rev")]
    copy_from_revision: Option<u64>,
}
pub fn parse_log(xml: &str) -> Result<Vec<LogEntry>, String> {
    let parsed: LogXml = parse_xml(xml, "log")?;
    Ok(parsed
        .entries
        .into_iter()
        .map(|e| LogEntry {
            revision: e.revision,
            author: e.author,
            date: e.date,
            message: e.message,
            paths: e
                .paths
                .paths
                .into_iter()
                .map(|p| LogPath {
                    path: p.path,
                    action: p.action,
                    kind: p.kind,
                    copy_from_path: p.copy_from_path,
                    copy_from_revision: p.copy_from_revision,
                })
                .collect(),
        })
        .collect())
}

pub fn has_special_property(xml: &str) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct Properties {
        #[serde(default, rename = "target")]
        targets: Vec<PropertyTarget>,
    }
    #[derive(Deserialize)]
    struct PropertyTarget {
        #[serde(default, rename = "property")]
        properties: Vec<PropertyName>,
    }
    #[derive(Deserialize)]
    struct PropertyName {
        #[serde(rename = "@name")]
        name: String,
    }
    let parsed: Properties = parse_xml(xml, "properties")?;
    if parsed.targets.len() > 1 {
        return Err("文件属性返回了多个目标，请刷新后重试。".into());
    }
    Ok(parsed
        .targets
        .iter()
        .flat_map(|target| &target.properties)
        .any(|property| property.name == "svn:special"))
}

pub fn command(
    action: &str,
    paths: &[String],
    message: &str,
    revision: Option<u64>,
) -> Result<Vec<String>, String> {
    if paths.len() > 200 {
        return Err("单次最多操作 200 个路径，请分批处理。".into());
    }
    let mut args: Vec<String> = match action {
        "status" => vec!["status", "--xml", "--ignore-externals"],
        "ignore-list" => vec!["proplist", "--xml", "--verbose", "--depth", "empty"],
        "properties" => vec!["proplist", "--xml", "--depth", "empty"],
        "info" => vec!["info", "--xml", "--depth", "empty"],
        "info-tree" => vec!["info", "--xml", "--depth", "infinity"],
        "log" => vec!["log", "--xml", "--verbose", "--limit", "50"],
        "add" => vec!["add", "--depth", "empty"],
        "delete" => vec!["delete"],
        "revert" => vec!["revert", "--depth", "empty"],
        "revert-tree" => vec!["revert", "--depth", "infinity"],
        "update" => vec!["update", "--ignore-externals"],
        "cleanup" => vec!["cleanup"],
        // Bridge/request strings are UTF-8. Never let SVN reinterpret the log
        // message using a GUI locale or the user's legacy log-encoding config.
        "commit" => vec![
            "commit",
            "--depth",
            "empty",
            "--encoding",
            "UTF-8",
            "--message",
        ],
        "cat-base" => vec!["cat", "--revision", "BASE"],
        "cat-revision" => vec!["cat", "--revision"],
        "resolve-mine" => vec!["resolve", "--accept", "mine-full", "--depth", "empty"],
        "resolve-theirs" => vec!["resolve", "--accept", "theirs-full", "--depth", "empty"],
        "resolve-working" => vec!["resolve", "--accept", "working", "--depth", "empty"],
        _ => return Err("不支持的 SVN 操作。".into()),
    }
    .into_iter()
    .map(str::to_owned)
    .collect();
    if action == "commit" {
        if message.trim().is_empty() || message.len() > 4096 || message.contains('\0') {
            return Err("请填写提交说明（最多 4096 字节）。".into());
        }
        args.push(message.to_owned());
    }
    if action == "log" {
        args.extend([
            "--revision".into(),
            format!(
                "{}:0",
                revision.map(|r| r.to_string()).unwrap_or("HEAD".into())
            ),
        ]);
    }
    if action == "cat-revision" {
        args.push(revision.ok_or("缺少版本号")?.to_string());
    }
    if paths.is_empty() && !matches!(action, "status" | "info" | "log" | "update" | "cleanup") {
        return Err("请选择操作目标。".into());
    }
    args.push("--non-interactive".into());
    args.push("--".into());
    if paths.is_empty() {
        args.push(".@".into());
    } else {
        for path in paths {
            args.push(target(path)?);
        }
    }
    Ok(args)
}

pub fn validate_commit(entries: &[Entry], selected: &[String]) -> Result<(), String> {
    use std::collections::HashSet;
    if selected.is_empty() {
        return Err("请勾选要提交的文件。".into());
    }
    let chosen: HashSet<&str> = selected.iter().map(String::as_str).collect();
    for path in selected {
        target(path)?;
        let entry = entries
            .iter()
            .find(|e| e.path == *path)
            .ok_or_else(|| format!("{path} 的状态已变化，请刷新后重新选择。"))?;
        if entry.status == "replaced" {
            return Err(format!(
                "{path} 是替换节点，旧目录可能包含未选中的子树。首版面板不直接提交替换节点；请还原后重新修改，或在 IDE 终端明确检查并提交。"
            ));
        }
        if matches!(entry.status.as_str(), "deleted" | "replaced")
            && (entry.kind == "unknown" || entry.kind == "dir" && !entry.metadata_complete)
        {
            return Err(format!("{path} 的目录信息不完整，请刷新后重试。"));
        }
        if entry.tree_conflicted
            || !matches!(
                entry.status.as_str(),
                "modified" | "added" | "deleted" | "replaced" | "normal"
            )
            || entry.status == "normal" && entry.props != "modified"
        {
            return Err(format!(
                "{path} 不能直接提交：{}。请先处理冲突、添加或缺失状态。",
                entry.status
            ));
        }
        if entry.kind == "dir"
            && entry.status == "deleted"
            && entries
                .iter()
                .any(|e| descendant(&e.path, path) && !chosen.contains(e.path.as_str()))
        {
            return Err(format!(
                "删除目录 {path} 会递归提交；请同时勾选其所有后代。"
            ));
        }
        for parent in entries
            .iter()
            .filter(|e| e.kind == "dir" && e.status == "added")
        {
            if descendant(path, &parent.path) && !chosen.contains(parent.path.as_str()) {
                return Err(format!("请同时勾选新增父目录 {}。", parent.path));
            }
        }
    }
    Ok(())
}

fn descendant(path: &str, parent: &str) -> bool {
    if parent == "." {
        path != "."
    } else {
        path.starts_with(&format!("{parent}/"))
    }
}

fn relative_xml_path(
    path: &str,
    root: Option<&str>,
    allow_management: bool,
) -> Result<String, String> {
    let normalized = if root.is_some_and(crate::workspace::windows_path) {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    let path = root
        .and_then(|root| crate::workspace::relative_path(root, &normalized))
        .unwrap_or(normalized);
    let path = path.strip_prefix("./").unwrap_or(&path);
    checked_target(path, allow_management)
        .map_err(|error| format!("SVN 返回的路径 {path:?} 无法归入工作目录 {root:?}：{error}"))?;
    Ok(path.to_owned())
}

fn validate_revert_node(entry: &Entry) -> Result<(), String> {
    if entry.status == "obstructed" {
        return Err(format!(
            "{} 被其他本地文件/目录阻塞，直接还原可能覆盖其中内容；请先移出阻塞项目。",
            entry.path
        ));
    }
    if (entry.copied || entry.replaced || entry.status == "replaced")
        && entry.kind != "file"
        && entry.kind != "symlink"
    {
        return Err(format!(
            "{} 是复制或替换目录，SVN 还原可能删除其中的本地内容。面板暂不直接还原此类目录；请先备份，再在 IDE 终端中明确处理。",
            entry.path
        ));
    }
    Ok(())
}
pub fn validate_revert_tree(entries: &[Entry], directory: &str) -> Result<(), String> {
    for entry in entries.iter().filter(|e| {
        directory == "." || e.path == directory || e.path.starts_with(&format!("{directory}/"))
    }) {
        validate_revert_node(entry)?;
    }
    Ok(())
}
