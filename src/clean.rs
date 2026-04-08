use anyhow::{bail, Result};
use chrono::NaiveDateTime;
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::cli::CleanMode;
use crate::config::Config;

/// 解析后的文件信息
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub env: Option<String>,
    pub timestamp: String,
}

/// 清理结果
#[derive(Debug)]
pub struct CleanResult {
    pub deleted: Vec<String>,
    pub warnings: Vec<String>,
}

/// 将 chrono date_format 转为正则表达式
///
/// 使用逐字符状态机：遇到 % 读取下一个字符查映射表，
/// 其余字面字符做 regex::escape。
pub fn date_format_to_regex(fmt: &str) -> String {
    let mut result = String::new();
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&next) = chars.peek() {
                chars.next();
                match next {
                    'Y' => result.push_str(r"\d{4}"),
                    'y' => result.push_str(r"\d{2}"),
                    'm' | 'd' | 'H' | 'M' | 'S' => result.push_str(r"\d{2}"),
                    'j' => result.push_str(r"\d{3}"),
                    'f' => result.push_str(r"\d+"),
                    '%' => result.push_str(&regex::escape("%")),
                    _ => {
                        // 未知格式符，fallback 为 .+
                        result.push_str(".+");
                    }
                }
            }
        } else {
            // 常见非元字符直接 push，避免逐字符分配
            if c.is_ascii_alphanumeric() || c == '_' {
                result.push(c);
            } else {
                result.push_str(&regex::escape(&c.to_string()));
            }
        }
    }

    result
}

/// 匹配 pakr 生成的文件名
///
/// - current 模式（env 已知）：构造完整正则精确匹配
/// - all 模式（env 未知）：两端夹逼 + chrono 二次验证
pub fn match_pakr_file(
    filename: &str,
    prefix: &str,
    sep: &str,
    date_format: &str,
    env: Option<&str>,
) -> Option<ParsedFile> {
    let ts_regex = date_format_to_regex(date_format);
    let p = regex::escape(prefix);
    let s = regex::escape(sep);

    match env {
        // current 模式：精确匹配
        Some(e) => {
            let e_escaped = regex::escape(e);
            let pattern = format!(r"^{p}{s}{e_escaped}{s}({ts_regex})\.zip$");
            let re = Regex::new(&pattern).ok()?;
            let caps = re.captures(filename)?;
            let ts = caps.get(1)?.as_str();

            // chrono 二次验证
            if !verify_timestamp(ts, date_format) {
                return None;
            }

            Some(ParsedFile {
                env: Some(e.to_string()),
                timestamp: ts.to_string(),
            })
        }
        // all 模式：两端夹逼
        None => {
            let pattern = format!(r"^{p}{s}(?:(.*){s})?({ts_regex})\.zip$");
            let re = Regex::new(&pattern).ok()?;
            let caps = re.captures(filename)?;

            let env_part = caps.get(1).map(|m| m.as_str().to_string());
            let ts = caps.get(2)?.as_str();

            if !verify_timestamp(ts, date_format) {
                return None;
            }

            Some(ParsedFile {
                env: env_part,
                timestamp: ts.to_string(),
            })
        }
    }
}

/// 用 chrono 验证 timestamp 字符串是否合法
fn verify_timestamp(ts: &str, date_format: &str) -> bool {
    // 尝试 NaiveDateTime 解析
    if NaiveDateTime::parse_from_str(ts, date_format).is_ok() {
        return true;
    }
    // 尝试 NaiveDate 解析（date_format 可能不含时间部分）
    if chrono::NaiveDate::parse_from_str(ts, date_format).is_ok() {
        return true;
    }
    false
}

/// 执行 clean 命令
///
/// `exclude`: pack 触发 clean 时传入的刚生成的文件名
/// `confirmed`: 是否已确认（all 模式需要确认）
pub fn clean_command(
    config: &Config,
    exclude: Option<&str>,
    confirmed: bool,
) -> Result<CleanResult> {
    let mut deleted = Vec::new();
    let mut warnings = Vec::new();

    // current 模式必须指定 env
    if config.cleanup.mode == CleanMode::Current && config.env.is_none() {
        bail!("current mode requires --env");
    }

    let output_dir = Path::new(&config.output);
    if !output_dir.exists() {
        // 目录不存在，无需清理
        return Ok(CleanResult { deleted, warnings });
    }

    // 扫描目录（非递归）
    let entries: Vec<_> = fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "zip")
        })
        .collect();

    // 匹配 pakr 生成的文件
    let env_for_match = match config.cleanup.mode {
        CleanMode::Current => config.env.as_deref(),
        CleanMode::All => None,
    };

    // 预编译正则，避免每个文件重复编译
    let ts_regex = date_format_to_regex(&config.date_format);
    let p = regex::escape(&config.prefix);
    let s = regex::escape(&config.separator);
    let pattern = match env_for_match {
        Some(e) => {
            let e_escaped = regex::escape(e);
            format!(r"^{p}{s}{e_escaped}{s}({ts_regex})\.zip$")
        }
        None => format!(r"^{p}{s}(?:(.*){s})?({ts_regex})\.zip$"),
    };
    let re = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(_) => return Ok(CleanResult { deleted: Vec::new(), warnings: Vec::new() }),
    };

    let mut matched: Vec<(String, std::time::SystemTime)> = Vec::new();

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().into_owned();

        if exclude.is_some_and(|exc| name == exc) {
            continue;
        }

        if let Some(caps) = re.captures(&name) {
            // chrono 二次验证 timestamp
            let ts_group = if env_for_match.is_some() { 1 } else { 2 };
            let valid = caps.get(ts_group).is_some_and(|m| verify_timestamp(m.as_str(), &config.date_format));
            if !valid {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            matched.push((name, mtime));
        }
    }

    // 确定要删除的文件
    let to_delete: Vec<String> = match config.cleanup.mode {
        CleanMode::Current => {
            // 按 mtime 倒序排列（最新在前），跳过前 keep 个
            matched.sort_by(|a, b| b.1.cmp(&a.1));
            matched
                .into_iter()
                .skip(config.cleanup.keep)
                .map(|(name, _)| name)
                .collect()
        }
        CleanMode::All => {
            if config.cleanup.keep != 1 {
                eprintln!("警告: --keep is ignored when --mode is all");
                warnings.push("--keep is ignored when --mode is all".to_string());
            }
            if !matched.is_empty() && !confirmed {
                bail!(
                    "all mode will delete {} files, use --force to confirm",
                    matched.len()
                );
            }
            matched.into_iter().map(|(name, _)| name).collect()
        }
    };

    if config.dry_run {
        for name in &to_delete {
            println!("[dry-run] 将删除: {}", name);
        }
        return Ok(CleanResult {
            deleted: to_delete,
            warnings,
        });
    }

    // 执行删除
    for name in &to_delete {
        let path = output_dir.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {
                println!("已删除: {}", name);
                deleted.push(name.clone());
            }
            Err(e) => {
                let msg = format!("删除失败: {} - {}", name, e);
                eprintln!("警告: {}", msg);
                warnings.push(msg);
            }
        }
    }

    Ok(CleanResult { deleted, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    // === date_format_to_regex 测试 ===

    #[test]
    fn test_regex_full_datetime() {
        let re = date_format_to_regex("%Y%m%d%H%M%S");
        assert_eq!(re, r"\d{4}\d{2}\d{2}\d{2}\d{2}\d{2}");
    }

    #[test]
    fn test_regex_with_literal_separators() {
        let re = date_format_to_regex("%Y-%m-%d");
        assert_eq!(re, r"\d{4}\-\d{2}\-\d{2}");
    }

    #[test]
    fn test_regex_short_format() {
        let re = date_format_to_regex("%m%d%H%M%S");
        assert_eq!(re, r"\d{2}\d{2}\d{2}\d{2}\d{2}");
    }

    #[test]
    fn test_regex_escaped_percent() {
        let re = date_format_to_regex("%%");
        assert_eq!(re, r"%");
    }

    // === match_pakr_file current 模式测试 ===

    #[test]
    fn test_current_match_exact() {
        let result = match_pakr_file(
            "my-project-prod-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.env.as_deref(), Some("prod"));
        assert_eq!(parsed.timestamp, "20260407143020");
    }

    #[test]
    fn test_current_no_match_wrong_env() {
        let result = match_pakr_file(
            "my-project-test-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_current_no_match_wrong_prefix() {
        let result = match_pakr_file(
            "other-project-prod-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_current_no_match_not_zip() {
        let result = match_pakr_file(
            "my-project-prod-20260407143020.tar.gz",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_current_no_match_substring_prefix() {
        let result = match_pakr_file(
            "webapp-my-project-prod-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_none());
    }

    // === match_pakr_file all 模式测试 ===

    #[test]
    fn test_all_match_with_env() {
        let result = match_pakr_file(
            "my-project-prod-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            None,
        );
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.env.as_deref(), Some("prod"));
    }

    #[test]
    fn test_all_match_without_env() {
        let result = match_pakr_file(
            "my-project-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            None,
        );
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert!(parsed.env.is_none());
    }

    #[test]
    fn test_all_match_env_with_separator() {
        let result = match_pakr_file(
            "my-project-pre-prod-20260407143020.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            None,
        );
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.env.as_deref(), Some("pre-prod"));
    }

    #[test]
    fn test_all_no_match_random_file() {
        let result = match_pakr_file(
            "random-file.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_all_no_match_invalid_timestamp() {
        let result = match_pakr_file(
            "my-project-prod-notadate00000.zip",
            "my-project",
            "-",
            "%Y%m%d%H%M%S",
            None,
        );
        assert!(result.is_none());
    }

    // === regex escape 安全性测试 ===

    #[test]
    fn test_regex_escape_dot_in_prefix_no_match() {
        let result = match_pakr_file(
            "myXapp-prod-20260407143020.zip",
            "my.app",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_regex_escape_dot_in_prefix_match() {
        let result = match_pakr_file(
            "my.app-prod-20260407143020.zip",
            "my.app",
            "-",
            "%Y%m%d%H%M%S",
            Some("prod"),
        );
        assert!(result.is_some());
    }

    // === clean_command 测试 ===

    /// 创建模拟 zip 文件，带指定的 mtime 偏移
    fn create_mock_zip(dir: &Path, name: &str, age_ms: u64) {
        let path = dir.join(name);
        fs::write(&path, "mock zip content").unwrap();
        // 通过 sleep 控制 mtime 顺序
        if age_ms > 0 {
            thread::sleep(Duration::from_millis(age_ms));
        }
    }

    fn make_clean_config(dir: &Path, mode: CleanMode, env: Option<&str>, keep: usize) -> Config {
        Config {
            prefix: "app".to_string(),
            separator: "-".to_string(),
            date_format: "%Y%m%d%H%M%S".to_string(),
            source: "dist".to_string(),
            output: dir.to_str().unwrap().to_string(),
            cleanup: crate::config::CleanupConfig {
                enabled: true,
                mode,
                keep,
            },
            env: env.map(|s| s.to_string()),
            dry_run: false,
            no_clean: false,
            force: false,
        }
    }

    #[test]
    fn test_clean_current_keep_1() {
        let dir = tempdir().unwrap();

        // 3 个 prod 包（按时间顺序创建）
        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260402000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260403000000.zip", 50);
        // 2 个 test 包
        create_mock_zip(dir.path(), "app-test-20260401000000.zip", 0);
        create_mock_zip(dir.path(), "app-test-20260402000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::Current, Some("prod"), 1);
        let result = clean_command(&config, None, true).unwrap();

        assert_eq!(result.deleted.len(), 2);
        // 最新的 prod 包保留
        assert!(!result.deleted.contains(&"app-prod-20260403000000.zip".to_string()));
        // test 包不受影响
        assert!(dir.path().join("app-test-20260401000000.zip").exists());
        assert!(dir.path().join("app-test-20260402000000.zip").exists());
    }

    #[test]
    fn test_clean_current_keep_2() {
        let dir = tempdir().unwrap();

        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260402000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260403000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::Current, Some("prod"), 2);
        let result = clean_command(&config, None, true).unwrap();

        assert_eq!(result.deleted.len(), 1);
        assert!(result.deleted.contains(&"app-prod-20260401000000.zip".to_string()));
    }

    #[test]
    fn test_clean_current_requires_env() {
        let dir = tempdir().unwrap();
        let config = make_clean_config(dir.path(), CleanMode::Current, None, 1);
        let result = clean_command(&config, None, true);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("current mode requires --env"));
    }

    #[test]
    fn test_clean_all_mode() {
        let dir = tempdir().unwrap();

        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 0);
        create_mock_zip(dir.path(), "app-test-20260401000000.zip", 0);
        create_mock_zip(dir.path(), "app-20260401000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::All, None, 1);
        let result = clean_command(&config, None, true).unwrap();

        assert_eq!(result.deleted.len(), 3);
    }

    #[test]
    fn test_clean_all_mode_warns_keep_ignored() {
        let dir = tempdir().unwrap();
        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 0);

        let mut config = make_clean_config(dir.path(), CleanMode::All, None, 3);
        config.cleanup.keep = 3; // 用户显式设置了 keep
        let result = clean_command(&config, None, true).unwrap();

        assert!(result.warnings.iter().any(|w| w.contains("--keep is ignored")));
    }

    #[test]
    fn test_clean_all_mode_no_warn_default_keep() {
        let dir = tempdir().unwrap();
        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::All, None, 1); // 默认 keep=1
        let result = clean_command(&config, None, true).unwrap();

        assert!(!result.warnings.iter().any(|w| w.contains("--keep is ignored")));
    }

    #[test]
    fn test_clean_all_mode_requires_confirm() {
        let dir = tempdir().unwrap();
        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::All, None, 1);
        let result = clean_command(&config, None, false); // 未确认

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--force"));
    }

    #[test]
    fn test_clean_dry_run() {
        let dir = tempdir().unwrap();

        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260402000000.zip", 0);

        let mut config = make_clean_config(dir.path(), CleanMode::Current, Some("prod"), 1);
        config.dry_run = true;

        let result = clean_command(&config, None, true).unwrap();

        // dry-run 返回将要删除的文件但不实际删除
        assert_eq!(result.deleted.len(), 1);
        // 文件仍然存在
        assert!(dir.path().join("app-prod-20260401000000.zip").exists());
        assert!(dir.path().join("app-prod-20260402000000.zip").exists());
    }

    #[test]
    fn test_clean_exclude_file() {
        let dir = tempdir().unwrap();

        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260402000000.zip", 50);
        create_mock_zip(dir.path(), "app-prod-20260403000000.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::Current, Some("prod"), 1);
        // 排除最新的文件（模拟 pack 触发 clean 时传入刚生成的文件名）
        let result = clean_command(&config, Some("app-prod-20260403000000.zip"), true).unwrap();

        // 排除的文件不被删除，keep=1 保留次新的
        assert!(!result.deleted.contains(&"app-prod-20260403000000.zip".to_string()));
        assert!(dir.path().join("app-prod-20260403000000.zip").exists());
    }

    #[test]
    fn test_clean_ignores_non_pakr_zip() {
        let dir = tempdir().unwrap();

        create_mock_zip(dir.path(), "app-prod-20260401000000.zip", 0);
        create_mock_zip(dir.path(), "backup-manual.zip", 0);
        create_mock_zip(dir.path(), "other-project.zip", 0);

        let config = make_clean_config(dir.path(), CleanMode::All, None, 1);
        let result = clean_command(&config, None, true).unwrap();

        // 只删除 pakr 生成的文件
        assert_eq!(result.deleted.len(), 1);
        assert!(result.deleted.contains(&"app-prod-20260401000000.zip".to_string()));
        // 非 pakr 文件仍存在
        assert!(dir.path().join("backup-manual.zip").exists());
        assert!(dir.path().join("other-project.zip").exists());
    }

    #[test]
    fn test_clean_empty_directory() {
        let dir = tempdir().unwrap();

        let config = make_clean_config(dir.path(), CleanMode::All, None, 1);
        let result = clean_command(&config, None, true).unwrap();

        assert!(result.deleted.is_empty());
        assert!(result.warnings.is_empty());
    }
}
