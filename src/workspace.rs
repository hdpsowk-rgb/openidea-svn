//! Workspace roots are chosen from the invoking workspace, never other windows.
use openidea_plugin::{Error, Project, host};
use serde::Deserialize;

#[derive(Deserialize)]
struct Workspace {
    projects: Vec<String>,
}

pub fn roots(project: &Project) -> Result<Vec<String>, Error> {
    let root = project.root()?;
    let mut roots = Vec::new();
    if let Ok(text) = project.read_file(".openidea/workspace.json")
        && text.len() <= 128 * 1024
        && let Ok(workspace) = serde_json::from_str::<Workspace>(&text)
    {
        for path in workspace.projects.into_iter().take(200) {
            if let Some(path) = normalize(&root, &path)
                && !roots.contains(&path)
            {
                roots.push(path);
            }
        }
    }
    if roots.is_empty() {
        roots.push(root);
    }
    Ok(roots)
}
fn normalize(base: &str, path: &str) -> Option<String> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return None;
    }
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
    let full = if absolute {
        path
    } else {
        format!("{base}/{path}")
    };
    let mut parts = Vec::new();
    for part in full.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let prefix = if full.starts_with("//") {
        "//"
    } else if full.starts_with('/') {
        "/"
    } else {
        ""
    };
    Some(format!("{prefix}{}", parts.join("/")))
}

/// SVN/host paths use either separator on Windows. Protocol targets always
/// remain slash-separated and relative; Unix backslashes keep their meaning.
pub fn windows_path(path: &str) -> bool {
    path.starts_with("\\\\")
        || path.starts_with("//")
        || (path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && path.as_bytes().get(1) == Some(&b':')
            && path
                .as_bytes()
                .get(2)
                .is_some_and(|b| matches!(b, b'/' | b'\\')))
}

pub fn relative_path(root: &str, path: &str) -> Option<String> {
    let windows = windows_path(root);
    let root = if windows {
        root.replace('\\', "/")
    } else {
        root.to_owned()
    };
    let path = if windows {
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    let root = root.trim_end_matches('/');
    let same = |a: &str, b: &str| {
        if windows {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    if same(root, path.trim_end_matches('/')) {
        return Some(".".into());
    }
    let prefix = path.get(..root.len())?;
    if same(root, prefix) && path.as_bytes().get(root.len()) == Some(&b'/') {
        Some(path[root.len() + 1..].to_owned())
    } else {
        None
    }
}

pub struct Scope {
    pub project: Project,
    pub root: String,
    pub direct: bool,
}
impl Scope {
    pub fn new(project: Project, root: String) -> Self {
        let project = host::project_of(&root).unwrap_or(project);
        let direct = project
            .root()
            .is_ok_and(|p| relative_path(&root, &p).as_deref() == Some("."));
        Self {
            project,
            root,
            direct,
        }
    }
    pub fn root(&self) -> Result<String, Error> {
        Ok(self.root.clone())
    }
    pub fn absolute(&self, path: &str) -> String {
        if path == "." {
            self.root.clone()
        } else {
            format!("{}/{path}", self.root.trim_end_matches('/'))
        }
    }
    /// Only literal local targets after `--` are rebased. URL history targets
    /// and option values (including commit messages) are never rewritten.
    pub fn args(&self, args: &[String]) -> Vec<String> {
        if self.direct {
            return args.to_vec();
        }
        let separator = args.iter().rposition(|a| a == "--").unwrap_or(args.len());
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                if index > separator && !arg.contains("://") {
                    let path = arg.strip_suffix('@').unwrap_or(arg);
                    format!("{}@", self.absolute(path))
                } else {
                    arg.clone()
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    #[test]
    fn option_values_that_look_like_separators_are_not_targets() {
        let host = MockHost::install();
        let scope = Scope::new(host.invocation().project().unwrap(), "/second".into());
        let args = crate::svn::command("commit", &["a.txt".into()], "--", None).unwrap();
        let actual = scope.args(&args);
        assert!(actual.iter().any(|arg| arg == "--non-interactive"));
        assert_eq!(actual.last().unwrap(), "/second/a.txt@");
        assert!(actual.windows(2).any(|pair| pair == ["--message", "--"]));
    }
}
