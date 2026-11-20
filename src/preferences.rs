use openidea_plugin::{Error, host};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RefreshSettings {
    pub enabled: bool,
    pub interval_seconds: u32,
}
impl Default for RefreshSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_seconds: 30,
        }
    }
}
impl RefreshSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(5..=3600).contains(&self.interval_seconds) {
            return Err("自动刷新间隔须为 5–3600 秒的整数。".into());
        }
        Ok(())
    }
    pub fn load(workspace: &str) -> Self {
        host::storage::get(&format!("refresh-settings:{workspace}"))
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default()
    }
    pub fn save(&self, workspace: &str) -> Result<(), Error> {
        self.validate()?;
        host::storage::set(
            &format!("refresh-settings:{workspace}"),
            &serde_json::to_string(self).map_err(|e| Error::Failed(e.to_string()))?,
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeView {
    #[default]
    Tree,
    List,
}
impl ChangeView {
    pub fn load(workspace: &str) -> Self {
        host::storage::get(&format!("change-view:{workspace}"))
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(self, workspace: &str) -> Result<(), Error> {
        host::storage::set(
            &format!("change-view:{workspace}"),
            &serde_json::to_string(&self).map_err(|e| Error::Failed(e.to_string()))?,
        )
    }
}
