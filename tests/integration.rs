use pakr::cli::CleanMode;
use pakr::clean;
use pakr::config::{CleanupConfig, Config};
use pakr::pack;
use std::fs;
use tempfile::tempdir;

/// 创建一个带 dist 目录的测试环境，返回 Config
/// 使用包含纳秒的时间格式避免同名文件
fn setup_test_env(
    cleanup_enabled: bool,
    keep: usize,
    no_clean: bool,
) -> (tempfile::TempDir, Config) {
    let dir = tempdir().unwrap();
    let source = dir.path().join("dist");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("index.html"), "<h1>Test</h1>").unwrap();

    let config = Config {
        prefix: "app".to_string(),
        separator: "-".to_string(),
        date_format: "%Y%m%d%H%M%S%f".to_string(), // 含纳秒，确保唯一
        source: source.to_str().unwrap().to_string(),
        output: dir.path().to_str().unwrap().to_string(),
        cleanup: CleanupConfig {
            enabled: cleanup_enabled,
            mode: CleanMode::Current,
            keep,
        },
        env: Some("prod".to_string()),
        dry_run: false,
        no_clean,
        force: false,
    };

    (dir, config)
}

/// 统计目录中 .zip 文件数量
fn count_zips(dir: &std::path::Path) -> usize {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "zip")
        })
        .count()
}

#[test]
fn test_pack_then_auto_clean() {
    let (dir, config) = setup_test_env(true, 1, false);

    // 连续 pack 3 次，模拟日常构建
    let result1 = pack::pack_command(&config).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let result2 = pack::pack_command(&config).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    let result3 = pack::pack_command(&config).unwrap().unwrap();

    // 文件名应该各不相同
    assert_ne!(result1.filename, result2.filename);
    assert_ne!(result2.filename, result3.filename);

    // 此时有 3 个 zip
    assert_eq!(count_zips(dir.path()), 3);

    // 模拟 pack 触发 clean：排除刚生成的 result3，keep=1
    // 非排除文件有 result1 和 result2，保留最新 1 个（result2），删除 result1
    let clean_result =
        clean::clean_command(&config, Some(&result3.filename), true).unwrap();

    assert_eq!(clean_result.deleted.len(), 1);
    assert!(clean_result.deleted.contains(&result1.filename));

    // 最终剩 2 个 zip：result2（保留） + result3（排除项）
    assert_eq!(count_zips(dir.path()), 2);
    assert!(dir.path().join(&result2.filename).exists());
    assert!(dir.path().join(&result3.filename).exists());
}

#[test]
fn test_pack_no_clean_flag() {
    let (dir, config) = setup_test_env(true, 1, true); // no_clean = true

    // 两次 pack
    pack::pack_command(&config).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    pack::pack_command(&config).unwrap().unwrap();

    // no_clean 生效，不触发 clean，两个包都在
    assert_eq!(count_zips(dir.path()), 2);
}

#[test]
fn test_pack_cleanup_disabled() {
    let (dir, config) = setup_test_env(false, 1, false); // cleanup disabled

    // 两次 pack
    pack::pack_command(&config).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    pack::pack_command(&config).unwrap().unwrap();

    // cleanup.enabled = false，两个包都在
    assert_eq!(count_zips(dir.path()), 2);
}
