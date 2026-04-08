use anyhow::{bail, Result};
use std::fs;
use std::io;
use std::path::Path;
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::config::Config;

/// 生成 zip 文件名
pub fn generate_filename(prefix: &str, sep: &str, env: Option<&str>, timestamp: &str) -> String {
    match env {
        Some(e) => format!("{prefix}{sep}{e}{sep}{timestamp}.zip"),
        None => format!("{prefix}{sep}{timestamp}.zip"),
    }
}

/// 打包结果
pub struct PackResult {
    pub filename: String,
    pub size: u64,
}

/// 创建 zip 文件
pub fn create_zip(source_dir: &Path, output_path: &Path) -> Result<(u64, Vec<String>)> {
    let mut warnings = Vec::new();

    if !source_dir.exists() {
        bail!("源目录不存在: {}", source_dir.display());
    }

    // 检查源目录是否为空
    let entries: Vec<_> = WalkDir::new(source_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    if entries.is_empty() {
        warnings.push(format!("源目录为空: {}", source_dir.display()));
    }

    // 确保输出目录存在
    if let Some(parent) = output_path.parent().filter(|p| !p.exists()) {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in &entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(source_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        zip.start_file(&relative, options)?;
        let mut f = fs::File::open(path)?;
        io::copy(&mut f, &mut zip)?;
    }

    zip.finish()?;

    let size = fs::metadata(output_path)?.len();
    Ok((size, warnings))
}

/// 执行 pack 命令
pub fn pack_command(config: &Config) -> Result<Option<PackResult>> {
    let source_dir = Path::new(&config.source);

    // 生成时间戳
    let timestamp = chrono::Local::now()
        .format(&config.date_format)
        .to_string();

    // 生成文件名
    let filename = generate_filename(
        &config.prefix,
        &config.separator,
        config.env.as_deref(),
        &timestamp,
    );

    let output_dir = Path::new(&config.output);
    let output_path = output_dir.join(&filename);

    if config.dry_run {
        println!("[dry-run] 将生成: {}", output_path.display());
        return Ok(None);
    }

    let (size, warnings) = create_zip(source_dir, &output_path)?;

    for w in &warnings {
        eprintln!("警告: {}", w);
    }

    let size_mb = size as f64 / 1024.0 / 1024.0;
    println!("已创建: {} ({:.2} MB)", filename, size_mb);

    Ok(Some(PackResult { filename, size }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    // === 文件名生成测试 ===

    #[test]
    fn test_filename_with_env() {
        let result = generate_filename("my-project", "-", Some("prod"), "20260407143020");
        assert_eq!(result, "my-project-prod-20260407143020.zip");
    }

    #[test]
    fn test_filename_without_env() {
        let result = generate_filename("my-project", "-", None, "20260407143020");
        assert_eq!(result, "my-project-20260407143020.zip");
    }

    #[test]
    fn test_filename_custom_separator() {
        let result = generate_filename("my_project", "_", Some("prod"), "20260407143020");
        assert_eq!(result, "my_project_prod_20260407143020.zip");
    }

    #[test]
    fn test_filename_custom_date_format() {
        let result = generate_filename("my-project", "-", Some("prod"), "0407143020");
        assert_eq!(result, "my-project-prod-0407143020.zip");
    }

    #[test]
    fn test_filename_prefix_contains_separator() {
        let result = generate_filename("my-app", "-", Some("prod"), "20260407143020");
        assert_eq!(result, "my-app-prod-20260407143020.zip");
    }

    // === zip 压缩测试 ===

    #[test]
    fn test_create_zip_normal() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(&source).unwrap();

        let mut f = fs::File::create(source.join("index.html")).unwrap();
        writeln!(f, "<h1>Hello</h1>").unwrap();

        let mut f = fs::File::create(source.join("app.js")).unwrap();
        writeln!(f, "console.log('hello')").unwrap();

        let output = dir.path().join("test.zip");
        let (size, warnings) = create_zip(&source, &output).unwrap();

        assert!(output.exists());
        assert!(size > 0);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_create_zip_verify_contents() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(source.join("sub")).unwrap();

        fs::write(source.join("a.txt"), "aaa").unwrap();
        fs::write(source.join("sub/b.txt"), "bbb").unwrap();

        let output = dir.path().join("test.zip");
        create_zip(&source, &output).unwrap();

        // 验证 zip 内容
        let file = fs::File::open(&output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();

        assert_eq!(names, vec!["a.txt", "sub/b.txt"]);
    }

    #[test]
    fn test_create_zip_source_not_exists() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("nonexistent");
        let output = dir.path().join("test.zip");

        let result = create_zip(&source, &output);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("源目录不存在"));
    }

    #[test]
    fn test_create_zip_empty_source() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(&source).unwrap();

        let output = dir.path().join("test.zip");
        let (_size, warnings) = create_zip(&source, &output).unwrap();

        assert!(warnings.iter().any(|w| w.contains("源目录为空")));
    }

    #[test]
    fn test_create_zip_paths_use_forward_slash() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(source.join("css")).unwrap();
        fs::write(source.join("css/style.css"), "body {}").unwrap();

        let output = dir.path().join("test.zip");
        create_zip(&source, &output).unwrap();

        let file = fs::File::open(&output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let name = archive.by_index(0).unwrap().name().to_string();

        // 确保使用 / 而非 \
        assert!(!name.contains('\\'));
        assert_eq!(name, "css/style.css");
    }

    // === pack_command 测试 ===

    #[test]
    fn test_pack_command_basic() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("index.html"), "<h1>Hello</h1>").unwrap();

        let config = Config {
            prefix: "test-project".to_string(),
            separator: "-".to_string(),
            date_format: "%Y%m%d%H%M%S".to_string(),
            source: source.to_str().unwrap().to_string(),
            output: dir.path().to_str().unwrap().to_string(),
            cleanup: crate::config::CleanupConfig {
                enabled: false,
                mode: crate::cli::CleanMode::Current,
                keep: 1,
            },
            env: Some("prod".to_string()),
            dry_run: false,
            no_clean: false,
            force: false,
        };

        let result = pack_command(&config).unwrap().unwrap();
        assert!(result.filename.starts_with("test-project-prod-"));
        assert!(result.filename.ends_with(".zip"));
        assert!(result.size > 0);

        // 验证文件存在
        let output_path = dir.path().join(&result.filename);
        assert!(output_path.exists());
    }

    #[test]
    fn test_pack_command_dry_run() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("index.html"), "hello").unwrap();

        let config = Config {
            prefix: "test".to_string(),
            separator: "-".to_string(),
            date_format: "%Y%m%d%H%M%S".to_string(),
            source: source.to_str().unwrap().to_string(),
            output: dir.path().to_str().unwrap().to_string(),
            cleanup: crate::config::CleanupConfig {
                enabled: false,
                mode: crate::cli::CleanMode::Current,
                keep: 1,
            },
            env: Some("dev".to_string()),
            dry_run: true,
            no_clean: false,
            force: false,
        };

        let result = pack_command(&config).unwrap();
        assert!(result.is_none()); // dry-run 不生成文件

        // 确认没有生成 zip 文件
        let zips: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "zip"))
            .collect();
        assert!(zips.is_empty());
    }

    #[test]
    fn test_pack_command_overwrites_existing() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("dist");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.txt"), "small").unwrap();

        let config = Config {
            prefix: "test".to_string(),
            separator: "-".to_string(),
            date_format: "FIXED".to_string(), // 固定时间戳使文件名相同
            source: source.to_str().unwrap().to_string(),
            output: dir.path().to_str().unwrap().to_string(),
            cleanup: crate::config::CleanupConfig {
                enabled: false,
                mode: crate::cli::CleanMode::Current,
                keep: 1,
            },
            env: Some("prod".to_string()),
            dry_run: false,
            no_clean: false,
            force: false,
        };

        // 第一次打包
        let result1 = pack_command(&config).unwrap().unwrap();
        let size1 = result1.size;

        // 增加内容使文件更大
        fs::write(source.join("big.txt"), "x".repeat(10000)).unwrap();

        // 第二次打包（同名覆盖）
        let result2 = pack_command(&config).unwrap().unwrap();
        assert_eq!(result1.filename, result2.filename);
        assert!(result2.size > size1); // 文件更大了，说明被覆盖了
    }
}
