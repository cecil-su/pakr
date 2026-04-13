use anyhow::Result;
use clap::Parser;
use pakr::cli::{self, Cli, Commands, PackArgs};
use pakr::config::CliArgs;
use pakr::{clean, config, init, pack};
use std::io::{self, Write};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dry_run = cli.dry_run;
    let config_path = cli.config.clone();

    // 不传子命令时等同于 pack（使用默认参数）
    let command = cli.command.unwrap_or(Commands::Pack(PackArgs {
        env: None,
        prefix: None,
        source: None,
        output: None,
        separator: None,
        date_format: None,
        no_clean: false,
        cleanup: false,
        keep: None,
    }));

    match command {
        Commands::Pack(args) => {
            let cli_args = CliArgs {
                prefix: args.prefix,
                separator: args.separator,
                date_format: args.date_format,
                source: args.source,
                output: args.output,
                env: args.env,
                dry_run,
                no_clean: args.no_clean,
                force: false,
                keep: args.keep,
                mode: None,
                cleanup_enabled: if args.cleanup { Some(true) } else { None },
            };
            let config = config::Config::load(cli_args, &config_path)?;
            let warnings = config.validate()?;
            for w in &warnings {
                eprintln!("警告: {}", w);
            }

            // 执行 pack
            let result = pack::pack_command(&config)?;

            // pack 成功后触发 clean
            if let Some(ref pack_result) = result.filter(|_| config.cleanup.enabled && !config.no_clean && !config.dry_run) {
                let clean_result = clean::clean_command(
                    &config,
                    Some(&pack_result.filename),
                    config.force,
                )?;
                if !clean_result.deleted.is_empty() {
                    println!("已清理 {} 个旧包", clean_result.deleted.len());
                }
            }
        }
        Commands::Clean(args) => {
            let cli_args = CliArgs {
                prefix: args.prefix,
                separator: args.separator,
                date_format: args.date_format,
                source: None,
                output: args.output,
                env: args.env,
                dry_run,
                no_clean: false,
                force: args.force,
                keep: args.keep,
                mode: Some(args.mode),
                cleanup_enabled: Some(true),
            };
            let config = config::Config::load(cli_args, &config_path)?;
            let warnings = config.validate()?;
            for w in &warnings {
                eprintln!("警告: {}", w);
            }

            // all 模式在交互终端需要确认
            let confirmed = if config.cleanup.mode == cli::CleanMode::All && !config.force && !config.dry_run {
                eprint!("确认删除所有匹配的包？[y/N] ");
                io::stderr().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                input.trim().eq_ignore_ascii_case("y")
            } else {
                true
            };

            let result = clean::clean_command(&config, None, confirmed)?;
            if result.deleted.is_empty() && result.warnings.is_empty() {
                println!("无需清理");
            }
        }
        Commands::Init => {
            let cwd = std::env::current_dir()?;
            init::init_command(&cwd)?;
        }
    }

    Ok(())
}
