use std::path::PathBuf;

pub fn default_settings_path() -> PathBuf {
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("l3dg3rr").join("settings.json");
        }
    }

    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg_config_home)
            .join("l3dg3rr")
            .join("settings.json");
    }

    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("l3dg3rr")
            .join("settings.json");
    }

    PathBuf::from("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_path_has_settings_file_name() {
        let path = default_settings_path();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("settings.json")
        );
    }
}
