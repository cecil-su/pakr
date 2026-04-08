use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// 生成 pakr.toml 配置文件模板
fn generate_template(prefix: &str) -> String {
    format!(
        r#"# pakr 配置文件
# 详细说明请参考: https://github.com/cecil-su/pakr

# 项目名称前缀，用于 zip 文件命名（默认：当前目录名）
prefix = "{prefix}"

# 分隔符（默认：-）
# separator = "-"

# 时间格式，chrono 格式（默认：%Y%m%d%H%M%S）
# date_format = "%Y%m%d%H%M%S"

# 源目录（默认：dist）
# source = "dist"

# 输出目录（默认：当前目录）
# output = "."

# 清理配置
[cleanup]
# 是否在打包时自动清理旧包（默认：false）
enabled = false
# 清理模式: "all" = 删除所有旧包, "current" = 仅清理指定环境的旧包
mode = "current"
# 保留最新的 N 个包（仅 current 模式生效，最小值 1）
keep = 1
"#
    )
}

/// 执行 init 命令
pub fn init_command(output_dir: &Path) -> Result<()> {
    let config_path = output_dir.join("pakr.toml");

    // 获取当前目录名作为默认 prefix
    let prefix = output_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());

    let content = generate_template(&prefix);

    // create_new 原子性地防止覆盖已有文件
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config_path)
        .with_context(|| {
            if config_path.exists() {
                format!("pakr.toml already exists at {}", config_path.display())
            } else {
                format!("无法创建配置文件: {}", config_path.display())
            }
        })?;
    file.write_all(content.as_bytes())?;

    println!("已创建: {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_init_creates_config_file() {
        let dir = tempdir().unwrap();
        init_command(dir.path()).unwrap();

        let config_path = dir.path().join("pakr.toml");
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).unwrap();
        // 应包含注释和所有配置字段
        assert!(content.contains("prefix"));
        assert!(content.contains("separator"));
        assert!(content.contains("date_format"));
        assert!(content.contains("source"));
        assert!(content.contains("output"));
        assert!(content.contains("[cleanup]"));
        assert!(content.contains("enabled"));
        assert!(content.contains("mode"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn test_init_fails_if_exists() {
        let dir = tempdir().unwrap();
        // 先创建一个 pakr.toml
        fs::write(dir.path().join("pakr.toml"), "existing").unwrap();

        let result = init_command(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_init_template_is_valid_toml() {
        let dir = tempdir().unwrap();
        init_command(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join("pakr.toml")).unwrap();
        // 应该能被 toml crate 解析
        let parsed: Result<crate::config::FileConfig, _> = toml::from_str(&content);
        assert!(parsed.is_ok(), "生成的 toml 无法解析: {:?}", parsed.err());
    }
}
