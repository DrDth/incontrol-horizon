//! `Config`: the original's NeoForge server config, kept as `incontrol-server.toml` in the game's
//! `config/` folder.

/// The file, as NeoForge writes it with the defaults when it is missing.
pub const FILE: &str = "incontrol-server.toml";

/// What NeoForge writes for the original's spec.
pub const DEFAULT: &str = r#"#General settings
[general]
	#How many ticks that the count cache is kept alive. This is used to avoid recalculating the cache every tick. Use 0 to recalculate every tick.
	# Default: 10
	# Range: 0 ~ 10000
	cacheRetentionTicks = 10
	#The radius that is used for player specific conditions (to find the nearest player)
	# Default: 100
	# Range: 1 ~ 10000
	perPlayerRadius = 100
"#;

/// The settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// `general.cacheRetentionTicks`.
    pub cache_retention_ticks: i32,
    /// `general.perPlayerRadius`.
    pub per_player_radius: i32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cache_retention_ticks: 10,
            per_player_radius: 100,
        }
    }
}

impl Config {
    /// The settings in `text`; a value that is missing, not a number or out of its range keeps
    /// its default, as NeoForge corrects it.
    pub fn parse(text: &str) -> Config {
        let mut config = Config::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = header.trim().trim_matches('"').to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.split('#').next().unwrap_or("").trim();
            let Ok(n) = value.parse::<i32>() else {
                continue;
            };
            match (section.as_str(), key.trim()) {
                ("general", "cacheRetentionTicks") if (0..=10000).contains(&n) => {
                    config.cache_retention_ticks = n;
                }
                ("general", "perPlayerRadius") if (1..=10000).contains(&n) => {
                    config.per_player_radius = n;
                }
                _ => {}
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_read_back_and_bad_values_keep_them() {
        assert_eq!(Config::parse(DEFAULT), Config::default());
        let config = Config::parse("[general]\ncacheRetentionTicks = 0\nperPlayerRadius = 0\n");
        assert_eq!(config.cache_retention_ticks, 0);
        assert_eq!(config.per_player_radius, 100);
    }
}
