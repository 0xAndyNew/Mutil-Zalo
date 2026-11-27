use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_COLORS: &[&str] = &[
    "#0068FF", "#10B981", "#F59E0B", "#EF4444", "#8B5CF6", "#EC4899", "#06B6D4", "#6366F1",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub minimize_to_tray: bool,
    pub start_with_windows: bool,
    pub preload_all_accounts: bool,
    pub enable_notifications: bool,
    pub auto_hide_sidebar: bool,
    pub sidebar_collapsed: bool,
    pub theme: String,
    pub zoom_percent: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            minimize_to_tray: true,
            start_with_windows: false,
            preload_all_accounts: true,
            enable_notifications: true,
            auto_hide_sidebar: false,
            sidebar_collapsed: false,
            theme: "light".to_string(),
            zoom_percent: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub config_version: u32,
    pub active_account_id: Option<String>,
    pub accounts: Vec<Account>,
    pub settings: AppSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        let first = Account {
            id: "acc-default-1".to_string(),
            name: "Zalo Chính".to_string(),
            color: DEFAULT_COLORS[0].to_string(),
            note: "Tài khoản mặc định".to_string(),
        };
        Self {
            config_version: 2,
            active_account_id: Some(first.id.clone()),
            accounts: vec![first],
            settings: AppSettings::default(),
        }
    }
}

impl AppConfig {
    pub fn app_data_root() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Mutil-Zalo")
    }

    pub fn config_path() -> PathBuf {
        Self::app_data_root().join("config.json")
    }

    pub fn profiles_dir() -> PathBuf {
        Self::app_data_root().join("profiles")
    }

    pub fn profile_dir(account_id: &str) -> PathBuf {
        Self::profiles_dir().join(sanitize_id(account_id))
    }

    pub fn load_or_default() -> Self {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let mut cfg = match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str::<AppConfig>(&raw).unwrap_or_default(),
            Err(_) => AppConfig::default(),
        };
        cfg.normalize();
        cfg
    }

    pub fn normalize(&mut self) {
        // One-time upgrade for v1 configs so preload_all_accounts is enabled for instant switching & background notifications
        if self.config_version < 2 {
            self.settings.preload_all_accounts = true;
            self.config_version = 2;
        }
        if self.accounts.is_empty() {
            let def = AppConfig::default();
            self.accounts = def.accounts;
            self.active_account_id = def.active_account_id;
        }
        let active_valid = self
            .active_account_id
            .as_ref()
            .map(|id| self.accounts.iter().any(|a| &a.id == id))
            .unwrap_or(false);
        if !active_valid {
            self.active_account_id = self.accounts.first().map(|a| a.id.clone());
        }
        self.settings.zoom_percent = self.settings.zoom_percent.clamp(75, 150);
        if self.settings.theme != "dark" && self.settings.theme != "light" {
            self.settings.theme = "light".to_string();
        }
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path).or_else(|_| {
            // Fallback on Windows if target exists and rename fails
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)
        })
    }

    pub fn add_account(&mut self, name: String, note: String, color: String) -> Account {
        let trimmed_name = name.trim();
        let display_name = if trimmed_name.is_empty() {
            format!("Zalo {}", self.accounts.len() + 1)
        } else {
            trimmed_name.to_string()
        };

        let chosen_color = if color.trim().starts_with('#') && color.trim().len() >= 4 {
            color.trim().to_string()
        } else {
            DEFAULT_COLORS[self.accounts.len() % DEFAULT_COLORS.len()].to_string()
        };

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut id = format!("acc-{ts}");
        let mut suffix = 1u32;
        while self.accounts.iter().any(|a| a.id == id) {
            id = format!("acc-{ts}-{suffix}");
            suffix += 1;
        }

        let acc = Account {
            id: id.clone(),
            name: display_name,
            color: chosen_color,
            note: note.trim().to_string(),
        };
        self.accounts.push(acc.clone());
        self.active_account_id = Some(id);
        acc
    }

    pub fn update_account(&mut self, id: &str, name: String, note: String, color: String) -> bool {
        if let Some(acc) = self.accounts.iter_mut().find(|a| a.id == id) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                acc.name = trimmed.to_string();
            }
            acc.note = note.trim().to_string();
            if color.trim().starts_with('#') && color.trim().len() >= 4 {
                acc.color = color.trim().to_string();
            }
            return true;
        }
        false
    }

    /// Removes an account from the list. Keeps at least 1 account so the user is never stuck empty.
    pub fn remove_account(&mut self, id: &str) -> Result<Account, &'static str> {
        if self.accounts.len() <= 1 {
            return Err("Cần giữ lại ít nhất 1 tài khoản Zalo.");
        }
        let pos = self
            .accounts
            .iter()
            .position(|a| a.id == id)
            .ok_or("Không tìm thấy tài khoản.")?;
        let removed = self.accounts.remove(pos);
        if self.active_account_id.as_deref() == Some(id) {
            let next_idx = pos.min(self.accounts.len() - 1);
            self.active_account_id = Some(self.accounts[next_idx].id.clone());
        }
        Ok(removed)
    }

    pub fn set_active(&mut self, id: &str) -> bool {
        if self.accounts.iter().any(|a| a.id == id) {
            self.active_account_id = Some(id.to_string());
            true
        } else {
            false
        }
    }

    pub fn active_account(&self) -> Option<&Account> {
        let id = self.active_account_id.as_deref()?;
        self.accounts.iter().find(|a| a.id == id)
    }
}

pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_and_manage_accounts_preserves_state() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.accounts.len(), 1);
        let first_id = cfg.accounts[0].id.clone();

        let second = cfg.add_account(
            "Zalo Công việc".into(),
            "Nhóm dự án".into(),
            "#10B981".into(),
        );
        assert_eq!(cfg.accounts.len(), 2);
        assert_eq!(cfg.active_account_id.as_deref(), Some(second.id.as_str()));

        // Switch back and forth without deleting or modifying either account
        assert!(cfg.set_active(&first_id));
        assert_eq!(cfg.active_account_id.as_deref(), Some(first_id.as_str()));
        assert!(cfg.set_active(&second.id));
        assert_eq!(cfg.active_account_id.as_deref(), Some(second.id.as_str()));

        // Update account
        assert!(cfg.update_account(
            &second.id,
            "Zalo Kinh doanh".into(),
            "CSKH".into(),
            "#F59E0B".into(),
        ));
        assert_eq!(cfg.active_account().unwrap().name, "Zalo Kinh doanh");

        // Remove second account -> falls back to first
        let removed = cfg.remove_account(&second.id).unwrap();
        assert_eq!(removed.id, second.id);
        assert_eq!(cfg.active_account_id.as_deref(), Some(first_id.as_str()));

        // Cannot remove last remaining account
        assert!(cfg.remove_account(&first_id).is_err());
    }

    #[test]
    fn roundtrip_save_and_load() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "mutil-zalo-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = tmp_dir.join("config.json");
        let mut cfg = AppConfig::default();
        cfg.add_account("Zalo Phụ".into(), "Test".into(), "#EF4444".into());
        cfg.settings.theme = "dark".into();
        cfg.settings.zoom_percent = 115;
        cfg.save_to(&path).unwrap();

        let loaded = AppConfig::load_from(&path);
        assert_eq!(cfg, loaded);
        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
