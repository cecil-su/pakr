use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "pakr", about = "通用构建产物打包工具", version)]
pub struct Cli {
    /// 指定配置文件路径
    #[arg(long, default_value = "pakr.toml")]
    pub config: String,

    /// 预览操作，不实际执行
    #[arg(long, short = 'n')]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 将目录压缩为 zip 包
    Pack(PackArgs),
    /// 清理旧的 zip 包
    Clean(CleanArgs),
    /// 生成 pakr.toml 配置文件
    Init,
}

#[derive(Parser, Debug)]
pub struct PackArgs {
    /// 指定环境
    #[arg(long, short = 'e')]
    pub env: Option<String>,

    /// 项目名称前缀
    #[arg(long, short = 'p')]
    pub prefix: Option<String>,

    /// 源目录
    #[arg(long, short = 's')]
    pub source: Option<String>,

    /// 输出目录
    #[arg(long, short = 'o')]
    pub output: Option<String>,

    /// 分隔符
    #[arg(long)]
    pub separator: Option<String>,

    /// 时间格式（chrono 格式）
    #[arg(long)]
    pub date_format: Option<String>,

    /// 跳过自动清理
    #[arg(long)]
    pub no_clean: bool,
}

#[derive(Parser, Debug)]
pub struct CleanArgs {
    /// 指定环境
    #[arg(long, short = 'e')]
    pub env: Option<String>,

    /// 清理模式
    #[arg(long, default_value = "current")]
    pub mode: CleanMode,

    /// 保留最新 N 个包
    #[arg(long)]
    pub keep: Option<usize>,

    /// 跳过确认提示（用于 CI 环境）
    #[arg(long)]
    pub force: bool,

    /// 项目名称前缀
    #[arg(long, short = 'p')]
    pub prefix: Option<String>,

    /// 分隔符
    #[arg(long)]
    pub separator: Option<String>,

    /// 时间格式（chrono 格式）
    #[arg(long)]
    pub date_format: Option<String>,

    /// 输出目录
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}

#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum CleanMode {
    /// 删除所有匹配的旧包
    All,
    /// 仅清理指定环境的旧包
    Current,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// 辅助函数：模拟命令行参数解析
    fn parse(args: &[&str]) -> Cli {
        let mut full_args = vec!["pakr"];
        full_args.extend_from_slice(args);
        Cli::parse_from(full_args)
    }

    #[test]
    fn test_pack_with_env_and_prefix() {
        let cli = parse(&["pack", "--env", "prod", "--prefix", "myapp"]);
        match cli.command {
            Some(Commands::Pack(args)) => {
                assert_eq!(args.env.as_deref(), Some("prod"));
                assert_eq!(args.prefix.as_deref(), Some("myapp"));
            }
            _ => panic!("应解析为 Pack 子命令"),
        }
    }

    #[test]
    fn test_clean_with_mode_all_and_force() {
        let cli = parse(&["clean", "--mode", "all", "--force"]);
        match cli.command {
            Some(Commands::Clean(args)) => {
                assert_eq!(args.mode, CleanMode::All);
                assert!(args.force);
            }
            _ => panic!("应解析为 Clean 子命令"),
        }
    }

    #[test]
    fn test_init_command() {
        let cli = parse(&["init"]);
        assert!(matches!(cli.command, Some(Commands::Init)));
    }

    #[test]
    fn test_no_subcommand_defaults_to_none() {
        // 不传子命令时 command 为 None，main.rs 中将其视为 Pack
        let cli = parse(&[]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_short_options() {
        let cli = parse(&["pack", "-e", "prod", "-p", "myapp", "-s", "build", "-o", "out"]);
        let cli_global = parse(&["-n", "pack"]);
        match cli.command {
            Some(Commands::Pack(args)) => {
                assert_eq!(args.env.as_deref(), Some("prod"));
                assert_eq!(args.prefix.as_deref(), Some("myapp"));
                assert_eq!(args.source.as_deref(), Some("build"));
                assert_eq!(args.output.as_deref(), Some("out"));
            }
            _ => panic!("应解析为 Pack 子命令"),
        }
        assert!(cli_global.dry_run);
    }
}
