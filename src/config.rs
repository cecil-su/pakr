use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::Deserialize;
use std::path::Path;

use crate::cli::CleanMode;

/// toml 配置文件对应的结构体（所有字段 Option）
#[derive(Debug, Deserialize, Default)]
pub struct FileConfig {
    pub prefix: Option<String>,
    pub separator: Option<String>,
    pub date_format: Option<String>,
    pub source: Option<String>,
    pub output: Option<String>,
    pub cleanup: Option<CleanupFileConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CleanupFileConfig {
    pub enabled: Option<bool>,
    pub mode: Option<String>,
    pub keep: Option<usize>,
}

/// 清理配置（最终值）
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    pub enabled: bool,
    pub mode: CleanMode,
    pub keep: usize,
}

/// 最终合并后的配置
#[derive(Debug, Clone)]
pub struct Config {
    pub prefix: String,
    pub separator: String,
    pub date_format: String,
    pub source: String,
    pub output: String,
    pub cleanup: CleanupConfig,
    pub env: Option<String>,
    pub dry_run: bool,
    pub no_clean: bool,
    pub force: bool,
}

/// CLI 传入的参数（用于合并，所有字段 Option）
#[derive(Debug, Default)]
pub struct CliArgs {
    pub prefix: Option<String>,
    pub separator: Option<String>,
    pub date_format: Option<String>,
    pub source: Option<String>,
    pub output: Option<String>,
    pub env: Option<String>,
    pub dry_run: bool,
    pub no_clean: bool,
    pub force: bool,
    pub keep: Option<usize>,
    pub mode: Option<CleanMode>,
    pub cleanup_enabled: Option<bool>,
}

impl Config {
    /// 加载配置：CLI 参数 > 配置文件 > 内置默认值
    pub fn load(cli: CliArgs, config_path: &str) -> Result<Self> {
        // 读取配置文件
        let file_config = Self::load_file_config(config_path)?;

        // 获取默认 prefix（当前目录名）
        let default_prefix = std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "project".to_string());

        // 解析 cleanup mode
        let file_mode = file_config
            .cleanup
            .as_ref()
            .and_then(|c| c.mode.as_deref())
            .and_then(|m| match m {
                "all" => Some(CleanMode::All),
                "current" => Some(CleanMode::Current),
                _ => None,
            });

        Ok(Config {
            prefix: cli
                .prefix
                .or(file_config.prefix)
                .unwrap_or(default_prefix),
            separator: cli
                .separator
                .or(file_config.separator)
                .unwrap_or_else(|| "-".to_string()),
            date_format: cli
                .date_format
                .or(file_config.date_format)
                .unwrap_or_else(|| "%Y%m%d%H%M%S".to_string()),
            source: cli
                .source
                .or(file_config.source)
                .unwrap_or_else(|| "dist".to_string()),
            output: cli
                .output
                .or(file_config.output)
                .unwrap_or_else(|| ".".to_string()),
            cleanup: CleanupConfig {
                enabled: cli
                    .cleanup_enabled
                    .or(file_config.cleanup.as_ref().and_then(|c| c.enabled))
                    .unwrap_or(false),
                mode: cli
                    .mode
                    .or(file_mode)
                    .unwrap_or(CleanMode::Current),
                keep: cli
                    .keep
                    .or(file_config.cleanup.as_ref().and_then(|c| c.keep))
                    .unwrap_or(1),
            },
            env: cli.env,
            dry_run: cli.dry_run,
            no_clean: cli.no_clean,
            force: cli.force,
        })
    }

    /// 校验配置，返回警告列表
    pub fn validate(&self) -> Result<Vec<String>> {
        if self.prefix.is_empty() {
            bail!("prefix must not be empty");
        }
        if self.separator.is_empty() {
            bail!("separator must not be empty");
        }
        if self.cleanup.keep < 1 {
            bail!("keep must be at least 1");
        }

        // 检查 date_format 是否会生成含非法字符的文件名
        let sample_ts = Local::now().format(&self.date_format).to_string();
        let illegal_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        if sample_ts.chars().any(|c| illegal_chars.contains(&c)) {
            bail!(
                "date_format '{}' generates filename with illegal characters: '{}'",
                self.date_format,
                sample_ts
            );
        }

        let mut warnings = Vec::new();

        // 检查 output 是否为根目录
        let output_path = Path::new(&self.output);
        let is_root = output_path.parent().is_none()
            || (output_path.is_absolute() && output_path.components().count() == 1);
        if is_root {
            warnings.push(format!(
                "output directory '{}' is a filesystem root, this is risky",
                self.output
            ));
        }

        Ok(warnings)
    }

    /// 从配置文件加载，文件不存在返回默认空配置
    fn load_file_config(config_path: &str) -> Result<FileConfig> {
        let path = Path::new(config_path);
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let config: FileConfig = toml::from_str(&content)
                    .with_context(|| format!("配置文件格式错误: {}", config_path))?;
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
            Err(e) => Err(anyhow::anyhow!("无法读取配置文件: {} - {}", config_path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_defaults_without_file_or_cli() {
        let cli = CliArgs::default();
        let config = Config::load(cli, "nonexistent-pakr.toml").unwrap();

        assert_eq!(config.separator, "-");
        assert_eq!(config.date_format, "%Y%m%d%H%M%S");
        assert_eq!(config.source, "dist");
        assert_eq!(config.output, ".");
        assert!(!config.cleanup.enabled);
        assert_eq!(config.cleanup.mode, CleanMode::Current);
        assert_eq!(config.cleanup.keep, 1);
        // prefix 应该是当前目录名，不为空
        assert!(!config.prefix.is_empty());
    }

    #[test]
    fn test_file_config_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("pakr.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, r#"prefix = "myapp""#).unwrap();
        writeln!(f, r#"separator = "_""#).unwrap();

        let cli = CliArgs::default();
        let config = Config::load(cli, config_path.to_str().unwrap()).unwrap();

        assert_eq!(config.prefix, "myapp");
        assert_eq!(config.separator, "_");
        // 未设置的字段使用默认值
        assert_eq!(config.date_format, "%Y%m%d%H%M%S");
    }

    #[test]
    fn test_cli_overrides_file_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("pakr.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, r#"prefix = "myapp""#).unwrap();

        let cli = CliArgs {
            prefix: Some("other".to_string()),
            ..Default::default()
        };
        let config = Config::load(cli, config_path.to_str().unwrap()).unwrap();

        assert_eq!(config.prefix, "other");
    }

    #[test]
    fn test_three_layer_merge() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("pakr.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, r#"prefix = "file-prefix""#).unwrap();
        writeln!(f, r#"separator = "_""#).unwrap();
        writeln!(f, r#"source = "build""#).unwrap();

        let cli = CliArgs {
            prefix: Some("cli-prefix".to_string()),
            // separator 不传，应该用文件的 "_"
            // source 不传，应该用文件的 "build"
            ..Default::default()
        };
        let config = Config::load(cli, config_path.to_str().unwrap()).unwrap();

        assert_eq!(config.prefix, "cli-prefix"); // CLI > file
        assert_eq!(config.separator, "_"); // file > default
        assert_eq!(config.source, "build"); // file > default
        assert_eq!(config.date_format, "%Y%m%d%H%M%S"); // default
    }

    #[test]
    fn test_missing_config_file_uses_defaults() {
        let cli = CliArgs::default();
        let result = Config::load(cli, "/nonexistent/path/pakr.toml");
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_config_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("pakr.toml");
        std::fs::write(&config_path, "invalid {{{{ toml content").unwrap();

        let cli = CliArgs::default();
        let result = Config::load(cli, config_path.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_cleanup_merge() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("pakr.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, "[cleanup]").unwrap();
        writeln!(f, "enabled = true").unwrap();
        writeln!(f, "keep = 3").unwrap();

        let cli = CliArgs::default();
        let config = Config::load(cli, config_path.to_str().unwrap()).unwrap();

        assert!(config.cleanup.enabled);
        assert_eq!(config.cleanup.keep, 3);
        assert_eq!(config.cleanup.mode, CleanMode::Current); // 默认值
    }

    // 校验测试
    fn make_config(overrides: impl FnOnce(&mut Config)) -> Config {
        let cli = CliArgs {
            prefix: Some("test".to_string()),
            ..Default::default()
        };
        let mut config = Config::load(cli, "nonexistent.toml").unwrap();
        overrides(&mut config);
        config
    }

    #[test]
    fn test_validate_empty_prefix() {
        let config = make_config(|c| c.prefix = "".to_string());
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("prefix must not be empty"));
    }

    #[test]
    fn test_validate_empty_separator() {
        let config = make_config(|c| c.separator = "".to_string());
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("separator must not be empty"));
    }

    #[test]
    fn test_validate_keep_zero() {
        let config = make_config(|c| c.cleanup.keep = 0);
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("keep must be at least 1"));
    }

    #[test]
    fn test_validate_illegal_date_format() {
        let config = make_config(|c| c.date_format = "%H:%M:%S".to_string());
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("illegal characters"));
    }

    #[test]
    fn test_validate_root_output_warning() {
        let config = make_config(|c| c.output = "/".to_string());
        let warnings = config.validate().unwrap();
        assert!(warnings.iter().any(|w| w.contains("filesystem root")));
    }

    #[test]
    fn test_validate_normal_config_passes() {
        let config = make_config(|_| {});
        let warnings = config.validate().unwrap();
        assert!(warnings.is_empty());
    }
}
