//! Local svn:ignore values. Names are literal, never shell or glob input.
use crate::svn;
use serde::Deserialize;

#[derive(Deserialize)]
struct Properties {
    #[serde(default, rename = "target")]
    targets: Vec<Target>,
}
#[derive(Deserialize)]
struct Target {
    #[serde(default, rename = "property")]
    properties: Vec<Property>,
}
#[derive(Deserialize)]
struct Property {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@encoding")]
    encoding: Option<String>,
    #[serde(default, rename = "$text")]
    value: String,
}
pub fn parse(xml: &str) -> Result<Option<String>, String> {
    let properties: Properties = svn::parse_xml(xml, "properties")?;
    if properties.targets.len() > 1 {
        return Err("忽略规则返回了多个目录。".into());
    }
    let mut value = None;
    for property in properties.targets.into_iter().flat_map(|t| t.properties) {
        if property.name == "svn:ignore" {
            if property.encoding.is_some() || value.is_some() {
                return Err("忽略规则不是可编辑的 UTF-8 文本。".into());
            }
            validate(&property.value)?;
            value = Some(property.value);
        }
    }
    Ok(value)
}
pub fn validate(value: &str) -> Result<(), String> {
    if value.len() > 65536
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err("忽略规则须为最多 64 KiB 的文本。".into());
    }
    Ok(())
}
pub fn update(existing: Option<&str>, name: &str, remove: bool) -> Result<Option<String>, String> {
    svn::target(name)?;
    if matches!(name, "." | "..") || name.contains('/') {
        return Err("请选择单个文件或目录名称。".into());
    }
    let pattern: String = name
        .chars()
        .map(|c| match c {
            '*' => "[*]".into(),
            '?' => "[?]".into(),
            '[' => "[[]".into(),
            ']' => "[]]".into(),
            _ => c.to_string(),
        })
        .collect();
    let old = existing.unwrap_or("");
    validate(old)?;
    let matches = |line: &str| line.trim_end_matches(['\r', '\n']) == pattern;
    let present = old.split_inclusive('\n').any(matches);
    if remove {
        if !present {
            return Err("该项不是由本目录中的同名规则忽略，请检查通配符或全局规则。".into());
        }
        let value: String = old
            .split_inclusive('\n')
            .filter(|line| !matches(line))
            .collect();
        return Ok(if value.is_empty() { None } else { Some(value) });
    }
    let mut value = old.to_owned();
    if !present {
        if !value.is_empty() && !value.ends_with('\n') {
            value.push('\n');
        }
        value.push_str(&pattern);
        value.push('\n');
    }
    validate(&value)?;
    Ok(Some(value))
}
pub fn command(parent: &str, value_file: Option<&str>) -> Result<Vec<String>, String> {
    let mut args = if let Some(file) = value_file {
        if file.chars().any(char::is_control) {
            return Err("无效的忽略规则文件。".into());
        }
        vec![
            "propset".into(),
            "--file".into(),
            file.into(),
            "svn:ignore".into(),
        ]
    } else {
        vec!["propdel".into(), "svn:ignore".into()]
    };
    args.extend([
        "--depth".into(),
        "empty".into(),
        "--non-interactive".into(),
        "--".into(),
        svn::target(parent)?,
    ]);
    Ok(args)
}

/// SVN uses APR filename globs with flags=0: bytewise, case-sensitive, dotfiles included.
/// Keep this bounded dynamic program independent of the host shell/platform globbing.
pub fn matches_any(value: &str, name: &str) -> bool {
    value
        .lines()
        .any(|pattern| glob_matches(pattern.as_bytes(), name.as_bytes()))
}
fn glob_matches(pattern: &[u8], name: &[u8]) -> bool {
    let mut previous = vec![false; name.len() + 1];
    previous[0] = true;
    let mut i = 0;
    while i < pattern.len() {
        let token = pattern[i];
        i += 1;
        let mut next = vec![false; name.len() + 1];
        if token == b'*' {
            next[0] = previous[0];
            for j in 1..=name.len() {
                next[j] = previous[j] || next[j - 1];
            }
        } else {
            let mut ranges = Vec::new();
            let mut negate = false;
            let mut literal = token;
            if token == b'[' {
                if i < pattern.len() && matches!(pattern[i], b'!' | b'^') {
                    negate = true;
                    i += 1;
                }
                let mut first = true;
                let mut closed = false;
                while i < pattern.len() {
                    if pattern[i] == b']' && !first {
                        i += 1;
                        closed = true;
                        break;
                    }
                    first = false;
                    let mut start = pattern[i];
                    i += 1;
                    if start == b'\\' && i < pattern.len() {
                        start = pattern[i];
                        i += 1;
                    }
                    let mut end = start;
                    if i + 1 < pattern.len() && pattern[i] == b'-' && pattern[i + 1] != b']' {
                        i += 1;
                        end = pattern[i];
                        i += 1;
                        if end == b'\\' && i < pattern.len() {
                            end = pattern[i];
                            i += 1;
                        }
                    }
                    ranges.push((start, end));
                }
                if !closed {
                    return false;
                }
            } else if token == b'\\' && i < pattern.len() {
                literal = pattern[i];
                i += 1;
            }
            for j in 1..=name.len() {
                let hit = if token == b'?' {
                    true
                } else if token == b'[' {
                    ranges
                        .iter()
                        .any(|(a, b)| *a <= name[j - 1] && name[j - 1] <= *b)
                        != negate
                } else {
                    literal == name[j - 1]
                };
                next[j] = previous[j - 1] && hit;
            }
        }
        previous = next;
    }
    previous[name.len()]
}
