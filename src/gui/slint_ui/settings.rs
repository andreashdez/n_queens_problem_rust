//! Settings kept between launches: in a JSON file natively, in `localStorage` on the web.
//!
//! Settings saved by the earlier egui version of the GUI are not read.

use serde::{Deserialize, Serialize};

use crate::gui::state::SavedSettings;

/// The light or dark theme, or the operating system's choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    /// The index the `.slint` side uses: system, dark, light.
    pub(super) const fn index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Dark => 1,
            Self::Light => 2,
        }
    }

    pub(super) const fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::System,
        }
    }
}

/// Everything restored on the next launch.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredSettings {
    #[serde(flatten)]
    pub app: SavedSettings,
    pub theme: Theme,
    pub layout: Layout,
}

/// Window and panel sizes in logical pixels. Missing sizes keep the window's defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Layout {
    /// Width and height. The browser page sizes the window there, so it is not restored.
    pub window: Option<[f32; 2]>,
    pub chart_height: Option<f32>,
    pub history_width: Option<f32>,
}

/// Where settings are kept.
pub(super) enum Storage {
    /// Tests, and platforms without a place for settings, keep them for this session only.
    None,
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
    #[cfg(target_arch = "wasm32")]
    Browser,
}

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "n-queens-solver.settings";

impl Storage {
    /// The platform's usual place for app settings.
    pub(super) fn platform_default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            settings_path().map_or(Self::None, Self::File)
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Browsers can block storage, for example for local files.
            if local_storage().is_some() {
                Self::Browser
            } else {
                Self::None
            }
        }
    }

    /// The stored settings, or the defaults if there are none or they cannot be read.
    pub(super) fn load(&self) -> StoredSettings {
        self.read()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn read(&self) -> Option<String> {
        match self {
            Self::None => None,
            #[cfg(not(target_arch = "wasm32"))]
            Self::File(path) => std::fs::read_to_string(path).ok(),
            #[cfg(target_arch = "wasm32")]
            Self::Browser => local_storage()?.get_item(STORAGE_KEY).ok()?,
        }
    }

    /// Saves `json`. Failures only cost the settings, so they are logged and dropped.
    pub(super) fn write(&self, json: &str) {
        match self {
            Self::None => {}
            #[cfg(not(target_arch = "wasm32"))]
            Self::File(path) => {
                let written = path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(path, json));
                if let Err(error) = written {
                    log::warn!("could not save settings to {}: {error}", path.display());
                }
            }
            #[cfg(target_arch = "wasm32")]
            Self::Browser => {
                if let Some(storage) = local_storage()
                    && storage.set_item(STORAGE_KEY, json).is_err()
                {
                    log::warn!("could not save settings to localStorage");
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// `settings.json` in the platform's per-user configuration folder.
#[cfg(not(target_arch = "wasm32"))]
fn settings_path() -> Option<std::path::PathBuf> {
    use std::{env, path::PathBuf};

    let non_empty = |name| env::var_os(name).filter(|value| !value.is_empty());
    let folder = if cfg!(target_os = "macos") {
        PathBuf::from(non_empty("HOME")?).join("Library/Application Support/N-Queens Solver")
    } else if cfg!(windows) {
        PathBuf::from(non_empty("APPDATA")?).join("N-Queens Solver")
    } else {
        non_empty("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| non_empty("HOME").map(|home| PathBuf::from(home).join(".config")))?
            .join("n-queens-solver")
    };
    Some(folder.join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::config::GuiPreset;

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let stored = StoredSettings {
            app: SavedSettings {
                config: GuiPreset::Classic.config(7),
                log_conflicts: false,
                show_history: true,
            },
            theme: Theme::Light,
            layout: Layout {
                window: Some([1200.0, 800.0]),
                chart_height: Some(350.0),
                history_width: None,
            },
        };
        let json = serde_json::to_string(&stored).unwrap();
        let restored: StoredSettings = serde_json::from_str(&json).unwrap();
        assert!(restored.app.config == stored.app.config);
        assert!(!restored.app.log_conflicts);
        assert_eq!(restored.theme, Theme::Light);
        assert_eq!(restored.layout, stored.layout);

        let partial: StoredSettings = serde_json::from_str(r#"{"theme": "dark"}"#).unwrap();
        assert_eq!(partial.theme, Theme::Dark);
        assert!(partial.app.log_conflicts);
        assert_eq!(Storage::None.load().theme, Theme::System);
        for theme in [Theme::System, Theme::Dark, Theme::Light] {
            assert_eq!(Theme::from_index(theme.index()), theme);
        }
    }
}
