# Easy Package

Easy Package 是一个只读的本机开发环境管理器 MVP。它使用 Tauri 2、React、TypeScript、Rust 和 SQLite，统一发现并展示 Homebrew、npm、pnpm、Yarn、Bun、Cargo、uv、pip、RubyGems、Composer，以及用户明确选择目录中的 JavaScript / Python / Rust / Go 项目元数据与直接依赖洞察。

## 当前边界

- 首发支持 macOS；其他平台返回受控的“不支持”状态。
- 桌面窗口最小宽度为 880px，不提供移动端 Web 适配。
- 只执行版本、列表、更新检查、缓存路径等只读命令。
- 扫描按管理器、项目、健康报告阶段显示进度，可随时取消；取消不会覆盖上一次成功快照。
- 项目扫描可设置最大遍历深度与用户忽略目录；默认跳过 `node_modules`、`.git`、`target`、`dist`、`build`、`.venv` 与 `vendor`，并在扫描日志中说明跳过原因。
- 可通过系统保存对话框导出当前环境快照为 JSON 或 Markdown；报告会将用户主目录替换为 `~`，且不包含诊断原始输出。
- 环境页会解析 node、npm、pnpm、Python/pip、Ruby/gem、PHP、Composer 的 PATH 优先级与候选来源，并只读标记命令冲突、运行时路径不一致和重复 Node 全局工具。
- 不提供安装、升级、卸载、清理或任意 Shell 执行接口。
- Yarn 仅支持 Classic 全局包目录扫描；Yarn Berry 会显示为已发现，但不扫描全局包。
- 项目扫描会从 Yarn Classic、Yarn Berry 与文本 `bun.lock` 关联 JavaScript 直接依赖的锁定版本；`bun.lockb` 仅展示受控限制提示，不尝试解析二进制内容。
- RubyGems 只读取本机的全局 gem 列表，不检查更新或执行写操作。
- Composer 只读取 Composer Home 中的 `vendor/composer/installed.json` 全局元数据；缓存目录通过受控的只读配置查询取得，不执行 `composer global show` 或更新检查。
- 项目扫描只读取 manifest、锁文件和运行时声明，不解析完整依赖树。
- “依赖”页面索引 JavaScript、Python、Rust、Go、Ruby 与 PHP 的直接声明依赖；跨生态同名包不会合并，并会标记跨项目的版本范围分歧。
- 对 package-lock、pnpm-lock、yarn.lock、bun.lock、Cargo.lock、uv.lock、Gemfile.lock 与 composer.lock，应用会只读关联直接依赖的已解析版本；无法匹配时明确显示“未解析”，不推测版本。
- 项目扫描支持 Poetry、Pipenv 与 Go modules；Poetry/Pipenv 分别读取其锁文件，Go 以 go.mod 的模块选择版本作为已解析版本来源，不执行模块下载。
- 项目扫描支持 Ruby 的 Gemfile/Gemfile.lock 与 PHP 的 composer.json/composer.lock；仅解析直接声明及锁定版本，不执行 Ruby DSL、Bundler 或 Composer 命令。
- 项目扫描会识别 JavaScript 与 Cargo 工作区，并提示工作区内版本分歧、未声明版本及本地依赖引用；不解析完整依赖树。
- 扫描快照、扫描根目录和诊断日志只保存在本机 SQLite 中。

## 本地开发

要求：Node.js、pnpm 11、Rust 1.84+，以及 Tauri 2 的 macOS 系统依赖。

```bash
pnpm install
pnpm tauri dev
```

仅预览前端界面（使用与 Rust 返回结构一致的模拟数据）：

```bash
pnpm dev
```

## 验证

```bash
pnpm test
pnpm build
pnpm test:rust
pnpm check
pnpm build:desktop
pnpm test:e2e
```

`pnpm check` 会依次执行前端测试、前端构建、Rust 单元测试与 Rust 格式检查。
`pnpm build:desktop` 以 release 模式构建 Tauri 原生二进制，但通过 `--no-bundle` 保持不生成 `.app`、DMG 或安装包；产物位于已忽略的 `src-tauri/target/release/`。
GitHub Actions 会在 macOS 上对 `develop` 推送与 Pull Request 执行质量门禁和原生二进制构建。

`pnpm test:e2e` 通过 `tauri-driver` 驱动真实 Tauri 窗口，并以 Rust `e2e` feature 和 `EASY_PACKAGE_E2E=1` 返回固定扫描 fixture。它不属于常规 CI 门禁：`tauri-driver` 不支持 macOS，且 Linux WebKit 驱动与固定 fixture 的维护成本不适合当前 macOS-first MVP。需要发布级原生验证时，在受控 Linux 环境或后续稳定的 macOS 原生方案中手动运行；该模式不读取本机包管理器、扫描目录或 SQLite。

## 桌面烟测

原生构建或 `pnpm tauri dev` 成功，只证明应用可以构建和启动，不替代 GUI E2E 或真实使用验证。每次涉及扫描、依赖或界面改动时，在 1440px 窗口完成以下只读检查：

1. 确认概览页显示扫描结果，并验证软件包筛选与空态。
2. 在系统临时目录创建测试项目目录，添加后检查项目和依赖解析来源，再从扫描根目录移除。
3. 验证环境页的健康项与诊断日志，并在扫描中执行一次取消操作。
4. 调整扫描范围后确认项目与依赖洞察同步更新；导出一份报告，检查主目录已脱敏且不含诊断原始输出。
5. 删除临时测试目录，确认不会遗留扫描根目录或修改任何包管理器状态。

## 结构

- `src/`：React 页面、组件、Tauri API 封装和前端测试。
- `src-tauri/src/adapters/`：包管理器发现、命令执行与输出解析。
- `src-tauri/src/scan/`：项目扫描、直接依赖索引、PATH 检查和健康规则。
- `src-tauri/src/storage.rs`：SQLite 快照、根目录、扫描设置和日志存储。
- `src-tauri/src/commands.rs`：对前端开放的受控只读 Tauri command；报告写入仅可经系统保存对话框触发。

命令执行统一使用可执行文件路径与参数数组，设置 20 秒超时，不经过 Shell。诊断输出会替换用户主目录并限制长度。
