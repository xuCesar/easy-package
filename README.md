# Easy Package

Easy Package 是一个只读的本机开发环境管理器 MVP。它使用 Tauri 2、React、TypeScript、Rust 和 SQLite，统一发现并展示 Homebrew、npm、pnpm、Yarn、Bun、Cargo、uv、pip、RubyGems、Composer，以及用户明确选择目录中的 JavaScript / Python / Rust / Go 项目元数据与直接依赖洞察。

## 当前边界

- 首发支持 macOS；其他平台返回受控的“不支持”状态。
- 桌面窗口最小宽度为 880px，不提供移动端 Web 适配。
- 只执行版本、列表、更新检查、缓存路径等只读命令。
- 扫描按管理器、项目、健康报告阶段显示进度，可随时取消；取消不会覆盖上一次成功快照。
- 不提供安装、升级、卸载、清理或任意 Shell 执行接口。
- Yarn 仅支持 Classic 全局包目录扫描；Yarn Berry 会显示为已发现，但不扫描全局包。
- RubyGems 只读取本机的全局 gem 列表，不检查更新或执行写操作。
- Composer 只读取 Composer Home 中的 `vendor/composer/installed.json` 全局元数据；缓存目录通过受控的只读配置查询取得，不执行 `composer global show` 或更新检查。
- 项目扫描只读取 manifest、锁文件和运行时声明，不解析完整依赖树。
- “依赖”页面索引 JavaScript、Python、Rust、Go、Ruby 与 PHP 的直接声明依赖；跨生态同名包不会合并，并会标记跨项目的版本范围分歧。
- 对 package-lock、pnpm-lock、Cargo.lock、uv.lock、Gemfile.lock 与 composer.lock，应用会只读关联直接依赖的已解析版本；无法匹配时明确显示“未解析”，不推测版本。
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
```

`pnpm check` 会依次执行前端测试、前端构建、Rust 单元测试与 Rust 格式检查。
GitHub Actions 会在 macOS 上对 `develop` 推送与 Pull Request 执行同一质量门禁。

## 结构

- `src/`：React 页面、组件、Tauri API 封装和前端测试。
- `src-tauri/src/adapters/`：包管理器发现、命令执行与输出解析。
- `src-tauri/src/scan/`：项目扫描、直接依赖索引、PATH 检查和健康规则。
- `src-tauri/src/storage.rs`：SQLite 快照、根目录和日志存储。
- `src-tauri/src/commands.rs`：对前端开放的七个受控 Tauri command。

命令执行统一使用可执行文件路径与参数数组，设置 20 秒超时，不经过 Shell。诊断输出会替换用户主目录并限制长度。
