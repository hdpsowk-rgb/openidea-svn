use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub client: String,
    pub generation: u64,
    pub root: Option<String>,
    pub action: String,
    pub directory: Option<String>,
    pub settings: Option<crate::preferences::RefreshSettings>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub message: String,
    pub revision: Option<u64>,
    pub path: Option<String>,
    pub accept: Option<String>,
    #[serde(default)]
    pub append: bool,
}

impl Request {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 128 * 1024 {
            return Err("请求过大。".into());
        }
        let request: Self =
            serde_json::from_str(text).map_err(|e| format!("无效的面板请求：{e}"))?;
        if request.client.is_empty()
            || request.client.len() > 96
            || !request
                .client
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("无效的面板标识。".into());
        }
        if !matches!(
            request.action.as_str(),
            "hello"
                | "select-root"
                | "settings"
                | "refresh"
                | "poll"
                | "cancel"
                | "draft"
                | "log"
                | "diff"
                | "history-diff"
                | "open"
                | "add"
                | "delete"
                | "revert"
                | "update"
                | "cleanup"
                | "commit"
                | "resolve"
                | "authenticate"
        ) {
            return Err("不支持的面板操作。".into());
        }
        if request.action == "settings" {
            request
                .settings
                .as_ref()
                .ok_or("缺少刷新配置。")?
                .validate()?;
        }
        if request.paths.len() > 200 {
            return Err("单次最多操作 200 个路径。".into());
        }
        for path in &request.paths {
            crate::svn::target(path)?;
        }
        if request.message.len() > 4096 {
            return Err("提交说明最多 4096 字节。".into());
        }
        Ok(request)
    }
    pub fn check_project(&self, root: &str) -> Result<(), String> {
        if self.root.as_deref().is_some_and(|r| r != root) {
            return Err("项目已切换，请重新打开 SVN 面板。".into());
        }
        if self.root.is_none() && self.action != "hello" {
            return Err("请先连接当前项目。".into());
        }
        Ok(())
    }
}
pub fn append_bounded(buffer: &mut String, chunk: &str, max: usize) -> Result<(), String> {
    if buffer.len().saturating_add(chunk.len()) > max {
        return Err("SVN 输出超过大小限制，请缩小操作范围。".into());
    }
    buffer.push_str(chunk);
    Ok(())
}
pub fn check_dirty(dirty: &[String], paths: &[String], all: bool) -> Result<(), String> {
    check_dirty_case(dirty, paths, all, false)
}
pub fn check_dirty_case(
    dirty: &[String],
    paths: &[String],
    all: bool,
    ignore_case: bool,
) -> Result<(), String> {
    let fold = |path: &str| {
        if ignore_case {
            path.to_lowercase()
        } else {
            path.to_owned()
        }
    };
    let paths: Vec<_> = paths.iter().map(|p| fold(p)).collect();
    let affected: Vec<_> = dirty
        .iter()
        .filter(|dirty| {
            let dirty = fold(dirty);
            all || paths
                .iter()
                .any(|p| p == "." || dirty == *p || dirty.starts_with(&format!("{p}/")))
        })
        .collect();
    if !affected.is_empty() {
        return Err(format!(
            "以下文件有未保存修改，请先保存或处理后重试：\n{}",
            affected
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_rejects_injected_commands_and_cross_project_use() {
        let req =
            Request::parse(r#"{"client":"one","generation":2,"root":"/a","action":"refresh"}"#)
                .unwrap();
        req.check_project("/a").unwrap();
        assert!(req.check_project("/b").is_err());
        for action in ["arbitrary", "commit --force", ""] {
            let text =
                serde_json::json!({"client":"one","generation":2,"root":"/a","action":action})
                    .to_string();
            assert!(Request::parse(&text).is_err());
        }
        assert!(Request::parse(r#"{"client":"","generation":0,"action":"hello"}"#).is_err());
        assert!(
            Request::parse(
                r#"{"client":"a","generation":0,"action":"add","paths":["../outside"]}"#
            )
            .is_err()
        );
    }
    #[test]
    fn utf8_output_overflow_is_an_error_instead_of_a_partial_xml_success() {
        let mut buffer = String::new();
        append_bounded(&mut buffer, "中文", 6).unwrap();
        assert_eq!(buffer, "中文");
        assert!(append_bounded(&mut buffer, "x", 6).is_err());
        assert_eq!(buffer, "中文");
    }
    #[test]
    fn dirty_descendants_block_destructive_directory_actions() {
        let dirty = vec!["dir/a.txt".into(), "other.txt".into()];
        assert!(check_dirty(&dirty, &["dir".into()], false).is_err());
        assert!(check_dirty(&dirty, &["other.txt".into()], false).is_err());
        check_dirty(&dirty, &["safe.txt".into()], false).unwrap();
        assert!(check_dirty(&dirty, &[], true).is_err());
    }
}
