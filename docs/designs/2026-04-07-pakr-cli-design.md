# pakr - 通用构建产物打包工具

**日期:** 2026-04-07

## 背景

原版 `umi-plugin-build-zip` 是一个 Umi 框架插件，在构建完成后将 `dist` 目录压缩为 zip 包，支持环境命名和旧包清理。pakr 将其核心功能提取为独立的 Rust CLI 工具，不绑定任何框架或语言生态，适用于任何需要将目录打包为 zip 的场景（前端产物、后端构建、文档、静态资源等）。

## 讨论

### 关键决策

1. **纯 CLI 工具**，不做插件/库形式 — Rust 做独立工具最自然，脱离框架依赖通用性更强
2. **子命令结构**（pack / clean / init）而非单命令 — 职责分离，clean 可独立使用，默认命令等同 `pakr pack`
3. **配置文件 + 命令行覆盖** — 日常用 `pakr.toml` 省事，CI 或临时调整用命令行参数覆盖
4. **去掉 env_map** — 环境名直接通过 `--env` 传入，用什么就是什么，更简洁
5. **源目录默认 `dist`，可配置** — 覆盖最常见场景，需要时通过 `--source` 修改
6. **输出到项目根目录** — 与原版一致，可通过 `--output` 修改

### 排除的方案

- **watch 模式自动打包** — 复杂度高，边界情况多，杀鸡用牛刀
- **智能检测产物目录** — 过度设计，默认 `dist` 可配置已足够

## 方案

### CLI 结构

```
pakr                        # 等同于 pakr pack
pakr pack [options]          # 打包
pakr clean [options]         # 单独清理
pakr init                   # 生成 pakr.toml 配置文件
```

通用 options：
- `--env, -e <ENV>` — 指定环境
- `--config <PATH>` — 指定配置文件路径（默认 `./pakr.toml`）
- `--dry-run, -n` — 预览操作，不实际执行

pack 专属 options：
- `--prefix, -p <NAME>` — 项目前缀（默认：当前目录名）
- `--source, -s <DIR>` — 源目录（默认 `dist`）
- `--output, -o <DIR>` — 输出目录（默认 `.`）
- `--no-clean` — 跳过自动清理

pack 高级 options（也可在配置文件中设置）：
- `--separator <CHAR>` — 分隔符（默认 `-`）
- `--date-format <FMT>` — 时间格式（默认 `%Y%m%d%H%M%S`）

clean 专属 options：
- `--mode <all|current>` — 清理模式（默认 `current`）
- `--keep <N>` — 保留最新 N 个包（默认 `1`，最小值 `1`，仅 `current` 模式生效）
- `--force` — 跳过 `all` 模式的确认提示（用于 CI 环境）

### 配置文件 `pakr.toml`

```toml
# 项目名称前缀，用于 zip 文件命名（默认：当前目录名）
# prefix = "my-project"

# 分隔符（默认：-）
# separator = "-"

# 时间格式，chrono 格式（默认：%Y%m%d%H%M%S）
# date_format = "%m%d%H%M%S"

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
```

优先级链：CLI 参数 > pakr.toml > 内置默认值

### 核心流程

**pack 流程：**
1. 加载配置（toml → CLI 参数合并）
2. 解析 prefix（CLI `--prefix` > 配置文件 > 当前目录名）
3. 配置校验（prefix 和 separator 不为空，keep >= 1）
4. 检查源目录是否存在（不存在则报错退出，为空则警告并继续）
5. 生成文件名：`{prefix}{sep}{env}{sep}{timestamp}.zip`（无 `--env` 时跳过环境段）
6. 校验文件名不含非法字符
7. 压缩源目录为 zip
8. **打包成功后**，如果 cleanup 启用且非 `--no-clean`，执行清理（将刚生成的文件名传入 clean 作为排除项）
9. 输出结果信息（文件名、大小）

**clean 流程：**
1. 加载配置
2. 配置校验（prefix 和 separator 不为空）
3. 扫描输出目录（非递归），使用文件名匹配算法识别 pakr 生成的文件
4. 根据 mode 筛选：
   - `all`：删除所有匹配的包。交互式终端需确认（显示文件列表和数量），非 TTY 需 `--force`
   - `current`：**必须指定 `--env`**（否则报错），按环境过滤后按修改时间排序保留最新 N 个
5. 跳过排除列表中的文件（pack 触发 clean 时传入的刚生成的文件）
6. 删除并输出结果

### 文件名匹配算法

clean 流程需要从输出目录中准确识别 pakr 生成的 zip 文件。采用**分模式匹配**策略：

**`current` 模式 — 精确匹配：**

prefix、sep、env、date_format 全部已知，构造完整正则一步匹配：
```
^{escape(prefix)}{escape(sep)}{escape(env)}{escape(sep)}{ts_regex}\.zip$
```
零歧义，安全性最高。

**`all` 模式 — 两端夹逼 + 二次验证：**

1. 前端锚定：验证文件名以 `{escape(prefix)}{escape(sep)}` 开头
2. 后端锚定：将 `date_format` 转为正则，从末尾匹配 timestamp
3. 中间剩余部分即为 env 段（可能为空）
4. 用 chrono `parse_from_str` 做二次验证，确认 timestamp 是合法日期时间

**关键实现要求：**
- prefix、sep 构建正则前必须做 `regex::escape()`（prefix 可能含 `.` 等正则元字符）
- 正则必须使用 `^...$` 完整锚定，不使用子串匹配
- `date_format_to_regex` 用逐字符状态机实现（`%Y` → `\d{4}`、`%m` → `\d{2}` 等），对无法识别的格式符 fallback 为 `.+` 并警告

### 配置校验规则

在配置加载完成后、执行操作前，校验以下规则：

| 字段 | 规则 | 违反时行为 |
|------|------|-----------|
| prefix | 不为空字符串 | 报错退出 |
| separator | 不为空字符串 | 报错退出 |
| keep | >= 1 | 报错退出 |
| date_format | 生成的文件名不含 `<>:"/\|?*` | 报错退出 |
| output | 不为文件系统根目录 | 警告 |

### 边界情况处理

- **prefix 默认值：** 未指定时使用当前目录名（类似 `cargo init`）
- **`--env` 缺失 + `current` 模式：** 报错退出，提示 "current mode requires --env"
- **`--keep` + `--mode all` 组合：** 警告 "--keep is ignored when --mode is all"
- **同名文件已存在：** 覆盖旧文件
- **输出目录不存在：** 自动创建（mkdir -p 语义）
- **符号链接：** 默认不跟随（与 tar 默认行为一致）
- **Windows 路径：** zip entry 内部路径统一转换为 `/`
- **`--date-format` 含非法字符：** 生成文件名后校验，含 `<>:"/\|?*` 则报错
- **pack 触发 clean：** 刚生成的文件名加入排除列表，确保不会被 clean 删除
- **`all` 模式确认：** 交互式终端显示文件列表并要求确认，非 TTY 需 `--force` 标志

### 依赖选型

- **CLI 解析：** clap（derive 模式）
- **配置文件：** toml + serde
- **zip 压缩：** zip crate（流式写入，勿 read_to_end）
- **目录遍历：** walkdir（follow_links=false）
- **时间格式化：** chrono
- **正则匹配：** regex
- **错误处理：** anyhow

## 约束与非功能需求

- 仅支持 zip 格式，不做其他压缩格式
- 配置文件不存在时使用内置默认值，不报错
- 配置文件格式错误时报错退出并提示具体位置
- 源目录不存在时报错退出
- 单个旧包删除失败时警告但继续执行
- cleanup.enabled 默认为 false，需用户显式开启
- clap 字段使用 `Option<T>` 实现三层配置合并
- clean 匹配必须使用 `regex::escape` + `^$` 锚定，禁止子串匹配

## 架构

```
src/
  main.rs         # 入口，CLI 解析
  cli.rs          # clap 定义（子命令、参数）
  config.rs       # 配置加载、合并与校验
  pack.rs         # 打包逻辑
  clean.rs        # 清理逻辑（含文件名匹配算法）
  init.rs         # 生成配置文件
```

数据流：CLI args + pakr.toml → 合并为 Config → 校验 → 传入 pack/clean 执行。
