# pakr 实现计划

**目标:** 实现一个 Rust CLI 工具，将指定目录压缩为带命名规则的 zip 包，支持旧包清理
**架构:** 单 binary CLI，clap derive 子命令结构，toml 配置文件，三层配置合并。核心模块：cli → config → pack/clean/init
**技术栈:** Rust, clap, serde, toml, zip, walkdir, chrono, regex, anyhow
**设计文档:** docs/designs/2026-04-07-pakr-cli-design.md
**新增依赖:**

| crate | 版本 | 用途 | 许可证 |
|-------|------|------|--------|
| clap | 4.6.0 (derive) | CLI 解析 | MIT/Apache-2.0 |
| serde | 1.0.228 (derive) | 序列化 | MIT/Apache-2.0 |
| toml | 1.1.2 | 配置文件解析 | MIT/Apache-2.0 |
| zip | 8.5.1 | zip 压缩 | MIT |
| walkdir | 2.5.0 | 目录遍历 | MIT/Unlicense |
| chrono | 0.4.44 | 时间格式化 | MIT/Apache-2.0 |
| regex | 1.12.3 | 文件名匹配 | MIT/Apache-2.0 |
| anyhow | 1.0.102 | 错误处理 | MIT/Apache-2.0 |

**测试模式:** TDD

---

### Task 1: 项目初始化与 CLI 骨架  ✅

**文件:**
- 创建: `Cargo.toml`
- 创建: `src/main.rs`
- 创建: `src/cli.rs`

**Step 1: 初始化 Cargo 项目**

```bash
cd D:/Workspace/ai/pakr
cargo init --name pakr
```

编辑 `Cargo.toml`，添加所有依赖：
```toml
[dependencies]
clap = { version = "4.6.0", features = ["derive"] }
serde = { version = "1.0.228", features = ["derive"] }
toml = "1.1.2"
zip = "8.5.1"
walkdir = "2.5.0"
chrono = "0.4.44"
regex = "1.12.3"
anyhow = "1.0.102"
```

**Step 2: 写 CLI 定义测试**

在 `src/cli.rs` 中用 clap derive 定义：
- 顶层 `Cli` 结构体，含 `--config`、`--dry-run` 全局选项
- `Commands` 枚举：`Pack`、`Clean`、`Init`
- `Pack` 子命令：`--env`、`--prefix`、`--source`、`--output`、`--separator`、`--date-format`、`--no-clean`（全部 `Option<T>`）
- `Clean` 子命令：`--env`、`--mode`、`--keep`、`--force`、`--prefix`、`--separator`、`--date-format`、`--output`
- `CleanMode` 枚举（`All`、`Current`），实现 `ValueEnum`
- 默认命令：不传子命令时等同于 `Pack`

测试用例（`#[cfg(test)] mod tests`）：
1. `pakr pack --env prod --prefix myapp` 正确解析
2. `pakr clean --mode all --force` 正确解析
3. `pakr init` 正确解析
4. `pakr --env prod`（无子命令）等同于 `pakr pack --env prod`
5. 短选项 `-e prod -p myapp -s build -o out -n` 正确解析

**Step 3: 写最小实现**

实现 `src/cli.rs` 中的 clap 结构体定义。`src/main.rs` 调用 `Cli::parse()` 并 match 子命令打印占位信息。

**Step 4: 验证**
```bash
cargo test -- cli
cargo run -- --help
cargo run -- pack --help
cargo run -- clean --help
```
预期：所有测试通过，help 输出包含所有选项。

---

### Task 2: 配置加载与合并  ✅

**文件:**
- 创建: `src/config.rs`
- 修改: `src/main.rs`（引入 config 模块）

**Step 1: 写测试**

在 `src/config.rs` 中定义 `Config` 结构体（最终合并后的配置）和 `FileConfig` 结构体（toml 文件对应，所有字段 `Option<T>`）。

测试用例：
1. **无配置文件无 CLI 参数** → 使用内置默认值（prefix=当前目录名, sep=`-`, date_format=`%Y%m%d%H%M%S`, source=`dist`, output=`.`, cleanup.enabled=false, cleanup.mode=current, cleanup.keep=1）
2. **配置文件覆盖默认值** → toml 中 `prefix = "myapp"` 生效
3. **CLI 参数覆盖配置文件** → toml 中 `prefix = "myapp"`，CLI 传 `--prefix other`，最终为 `other`
4. **三层合并优先级** → CLI > toml > 默认值，逐字段验证
5. **配置文件不存在** → 不报错，使用默认值
6. **配置文件格式错误** → 返回错误
7. **cleanup 部分合并** → toml 设置 `enabled = true`，CLI 不传 cleanup 相关参数，enabled 为 true

**Step 2: 跑测试确认失败**
```bash
cargo test -- config
```
预期：FAIL（结构体未实现）

**Step 3: 写最小实现**

- `FileConfig`：用 `serde::Deserialize` 从 toml 反序列化，所有字段 `Option<T>`，含嵌套 `CleanupFileConfig`
- `Config`：最终配置，所有字段有值（非 Option）
- `Config::load(cli_args, config_path) -> Result<Config>`：
  1. 尝试读取 config_path，不存在则 FileConfig 全 None
  2. 解析 toml
  3. 逐字段合并：`cli.field.or(file.field).unwrap_or(default)`
  4. prefix 默认值用 `std::env::current_dir()` 取目录名

**Step 4: 跑测试确认通过**
```bash
cargo test -- config
```
预期：PASS

---

### Task 3: 配置校验  ✅

**文件:**
- 修改: `src/config.rs`

**Step 1: 写测试**

测试用例：
1. **prefix 为空** → 返回错误 "prefix must not be empty"
2. **separator 为空** → 返回错误 "separator must not be empty"
3. **keep = 0** → 返回错误 "keep must be at least 1"
4. **date_format 生成含非法字符的文件名**（如 `%H:%M:%S`）→ 返回错误
5. **output 为根目录** → 返回警告（不阻塞，打印 warning）
6. **正常配置** → 校验通过

**Step 2: 跑测试确认失败**
```bash
cargo test -- config::tests::validate
```
预期：FAIL

**Step 3: 写最小实现**

在 `Config` 上添加 `validate(&self) -> Result<Vec<Warning>>`：
- 检查 prefix 非空
- 检查 separator 非空
- 检查 keep >= 1
- 用 chrono 格式化当前时间，检查结果是否含 `<>:"/\|?*`
- 检查 output 是否为根目录（`/`、`C:\` 等），是则返回 warning

**Step 4: 跑测试确认通过**
```bash
cargo test -- config::tests::validate
```
预期：PASS

---

### Task 4: 文件名生成  ✅

**文件:**
- 创建: `src/pack.rs`

**Step 1: 写测试**

测试 `generate_filename` 函数：
1. **有 env** → `my-project-prod-20260407143020.zip`
2. **无 env** → `my-project-20260407143020.zip`
3. **自定义 separator** → `my_project_prod_20260407143020.zip`（sep=`_`）
4. **自定义 date_format** → `my-project-prod-0407143020.zip`（format=`%m%d%H%M%S`）
5. **prefix 含分隔符字符** → `my-app-prod-20260407143020.zip`（prefix=`my-app`, sep=`-`）

**Step 2: 跑测试确认失败**
```bash
cargo test -- pack::tests
```
预期：FAIL

**Step 3: 写最小实现**

```rust
fn generate_filename(prefix: &str, sep: &str, env: Option<&str>, timestamp: &str) -> String {
    match env {
        Some(e) => format!("{prefix}{sep}{e}{sep}{timestamp}.zip"),
        None => format!("{prefix}{sep}{timestamp}.zip"),
    }
}
```

测试中需要 mock 时间戳（传入固定的 timestamp 字符串而非调用 chrono），以保证测试确定性。

**Step 4: 跑测试确认通过**
```bash
cargo test -- pack::tests
```
预期：PASS

---

### Task 5: zip 压缩逻辑  ✅

**文件:**
- 修改: `src/pack.rs`

**Step 1: 写测试**

测试 `create_zip` 函数（使用 tempdir）：
1. **正常压缩** → 在 tempdir 创建几个文件，压缩后验证 zip 文件存在且大小 > 0
2. **验证 zip 内容** → 解压后文件列表与源目录一致
3. **源目录不存在** → 返回错误
4. **源目录为空** → 生成 zip 并返回警告
5. **Windows 路径转换** → zip entry 中的路径使用 `/` 而非 `\`

**Step 2: 跑测试确认失败**
```bash
cargo test -- pack::tests
```
预期：FAIL

**Step 3: 写最小实现**

- 使用 `walkdir` 遍历源目录（`follow_links(false)`）
- 对每个文件用 `std::io::copy` 流式写入 zip entry
- zip entry 路径：计算相对路径后将 `\` 替换为 `/`
- 输出目录不存在时 `fs::create_dir_all`

**Step 4: 跑测试确认通过**
```bash
cargo test -- pack::tests
```
预期：PASS

---

### Task 6: pack 命令完整流程  ✅

**文件:**
- 修改: `src/pack.rs`
- 修改: `src/main.rs`

**Step 1: 写测试**

测试 `pack_command(config) -> Result<PackResult>`：
1. **基本流程** → 创建 tempdir 含 dist/ 子目录和文件，执行 pack，验证 zip 生成在正确位置，文件名格式正确
2. **dry-run 模式** → 不生成 zip 文件，仅打印预期文件名
3. **同名文件覆盖** → 已存在同名 zip，执行 pack 后文件被覆盖（大小不同）

**Step 2: 跑测试确认失败**
```bash
cargo test -- pack::tests
```
预期：FAIL

**Step 3: 写最小实现**

组装完整的 pack 流程：
1. 配置已由 main.rs 加载并校验
2. 生成时间戳（`chrono::Local::now().format(date_format)`）
3. 调用 `generate_filename`
4. 如果 dry-run，打印信息并返回
5. 调用 `create_zip`
6. 打印结果（文件名、大小 MB）
7. 返回 `PackResult { filename, size }`（供 clean 排除用）

在 `main.rs` 中接入 pack 子命令。

**Step 4: 跑测试确认通过**
```bash
cargo test -- pack::tests
```
预期：PASS

**手动验证：**
```bash
mkdir -p /tmp/pakr-test/dist && echo "hello" > /tmp/pakr-test/dist/index.html
cd /tmp/pakr-test && cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack --env prod --prefix test-project --source dist
```
预期：生成 `test-project-prod-XXXXXXXXXXXXXX.zip`

---

### Task 7: 文件名匹配算法  ✅

**文件:**
- 创建: `src/clean.rs`

这是安全性最关键的模块，需要充分的测试覆盖。

**Step 1: 写测试**

测试 `date_format_to_regex` 函数：
1. `%Y%m%d%H%M%S` → `\d{4}\d{2}\d{2}\d{2}\d{2}\d{2}`
2. `%Y-%m-%d` → `\d{4}-\d{2}-\d{2}`（字面字符 `-` 被 escape）
3. `%m%d%H%M%S` → `\d{2}\d{2}\d{2}\d{2}\d{2}`
4. `%%` → `%`（转义百分号）

测试 `match_pakr_file` 函数（current 模式精确匹配）：
5. `my-project-prod-20260407143020.zip`（prefix=`my-project`, sep=`-`, env=`prod`）→ 匹配
6. `my-project-test-20260407143020.zip`（env=`prod`）→ 不匹配（环境不同）
7. `other-project-prod-20260407143020.zip`（prefix=`my-project`）→ 不匹配（prefix 不同）
8. `my-project-prod-20260407143020.tar.gz` → 不匹配（非 zip）
9. `webapp-my-project-prod-20260407143020.zip` → 不匹配（前缀被包含但非锚定）

测试 `match_pakr_file` 函数（all 模式两端夹逼）：
10. `my-project-prod-20260407143020.zip` → 匹配，env=`prod`
11. `my-project-20260407143020.zip`（无 env）→ 匹配，env=None
12. `my-project-pre-prod-20260407143020.zip` → 匹配，env=`pre-prod`
13. `random-file.zip` → 不匹配
14. `my-project-prod-notadate.zip` → 不匹配（chrono 二次验证失败）

测试 regex escape 安全性：
15. prefix=`my.app`（含正则元字符 `.`）→ 不匹配 `myXapp-prod-20260407143020.zip`
16. prefix=`my.app` → 匹配 `my.app-prod-20260407143020.zip`

**Step 2: 跑测试确认失败**
```bash
cargo test -- clean::tests
```
预期：FAIL

**Step 3: 写最小实现**

- `date_format_to_regex(fmt: &str) -> String`：逐字符状态机，遇到 `%` 读取下一个字符查映射表，其余字符做 `regex::escape`
- `match_pakr_file(filename, prefix, sep, date_format, env: Option<&str>) -> Option<ParsedFile>`：
  - 有 env（current 模式）：构造完整正则精确匹配
  - 无 env（all 模式）：前端锚定 prefix，后端用 ts_regex 匹配，chrono 二次验证
  - 所有用户输入（prefix、sep、env）做 `regex::escape`

**Step 4: 跑测试确认通过**
```bash
cargo test -- clean::tests
```
预期：PASS

---

### Task 8: clean 命令完整流程  ✅

**文件:**
- 修改: `src/clean.rs`
- 修改: `src/main.rs`

**Step 1: 写测试**

使用 tempdir 创建模拟的 zip 文件（空文件即可，clean 只看文件名和 mtime）。

测试 `clean_command` 函数：
1. **current 模式 keep=1** → 3 个 prod 包 + 2 个 test 包，env=prod，删除 2 个 prod 旧包，test 包不受影响
2. **current 模式 keep=2** → 3 个 prod 包，删除 1 个最旧的
3. **current 模式无 --env** → 返回错误 "current mode requires --env"
4. **all 模式** → 删除所有匹配的包（测试中跳过确认）
5. **all 模式 + --keep** → 返回警告信息
6. **dry-run 模式** → 不删除文件，返回将要删除的文件列表
7. **排除列表** → 传入排除文件名，该文件不被删除
8. **目录中有非 pakr 的 zip** → 不被匹配，不被删除
9. **目录为空** → 无操作，不报错

**Step 2: 跑测试确认失败**
```bash
cargo test -- clean::tests
```
预期：FAIL

**Step 3: 写最小实现**

```rust
pub fn clean_command(config: &Config, exclude: Option<&str>) -> Result<CleanResult>
```

流程：
1. 读取 output 目录（`fs::read_dir`，非递归）
2. 对每个 `.zip` 文件调用 `match_pakr_file` 判断是否匹配
3. 过滤掉排除列表中的文件
4. 根据 mode 筛选要删除的文件：
   - `current`：匹配到的且 env 相同的，按 mtime 排序，保留最新 keep 个
   - `all`：所有匹配到的（交互确认逻辑在此任务中实现为接受一个 `confirm: bool` 参数，由 main.rs 处理 TTY 检测和用户交互）
5. 如果 dry-run，打印列表并返回
6. 逐个删除，单个失败警告不中断
7. 返回 `CleanResult { deleted, skipped, warnings }`

在 `main.rs` 中接入 clean 子命令，处理 TTY 检测和确认提示。

**Step 4: 跑测试确认通过**
```bash
cargo test -- clean::tests
```
预期：PASS

---

### Task 9: pack + clean 联动  ✅

**文件:**
- 修改: `src/main.rs`
- 修改: `src/pack.rs`

**Step 1: 写测试**

集成测试（`tests/integration.rs`）：
1. **pack 后自动 clean** → 配置 cleanup.enabled=true, keep=1，连续 pack 两次（相同 env），验证只保留最新 1 个包 + 刚生成的包
2. **pack --no-clean** → cleanup.enabled=true，但传 --no-clean，旧包不被删除
3. **pack 不触发 clean（cleanup disabled）** → cleanup.enabled=false，旧包不被删除

**Step 2: 跑测试确认失败**
```bash
cargo test --test integration
```
预期：FAIL

**Step 3: 写最小实现**

在 pack 流程步骤 8 中：
- 检查 `config.cleanup.enabled && !config.no_clean`
- 调用 `clean_command(config, Some(&new_filename))`

**Step 4: 跑测试确认通过**
```bash
cargo test --test integration
```
预期：PASS

---

### Task 10: init 命令  ✅

**文件:**
- 创建: `src/init.rs`
- 修改: `src/main.rs`

**Step 1: 写测试**

测试用例：
1. **生成配置文件** → 在 tempdir 执行 init，验证 `pakr.toml` 存在且内容包含注释和所有配置字段
2. **配置文件已存在** → 报错 "pakr.toml already exists, use --force to overwrite"（不覆盖）
3. **生成的 toml 可被 toml crate 解析** → 反序列化为 FileConfig 不报错

**Step 2: 跑测试确认失败**
```bash
cargo test -- init::tests
```
预期：FAIL

**Step 3: 写最小实现**

`init_command` 生成带注释的 `pakr.toml` 模板（所有配置项注释掉，cleanup section 取消注释但 enabled=false）。prefix 使用当前目录名填入。

**Step 4: 跑测试确认通过**
```bash
cargo test -- init::tests
```
预期：PASS

**手动验证：**
```bash
cd /tmp/pakr-test && cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- init
cat pakr.toml
```

---

### Task 11: 端到端验证与收尾  ✅

**文件:**
- 修改: `src/main.rs`（完善错误输出和退出码）

**Step 1: 全量测试**
```bash
cargo test
cargo clippy
```
预期：所有测试通过，无 clippy 警告

**Step 2: 端到端手动验证**

```bash
# 创建测试项目
mkdir -p /tmp/e2e-pakr/dist && echo "<h1>Hello</h1>" > /tmp/e2e-pakr/dist/index.html
cd /tmp/e2e-pakr

# init
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- init

# pack
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack --env prod
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack --env test
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack  # 无 env

# dry-run
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack --env prod --dry-run

# clean
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- clean --mode current --env prod --keep 1
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- clean --mode all --force

# 验证 help
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- --help
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- pack --help
cargo run --manifest-path D:/Workspace/ai/pakr/Cargo.toml -- clean --help
```

**Step 3: 确保构建成功**
```bash
cargo build --release
```
预期：编译成功，生成 `target/release/pakr` 可执行文件
