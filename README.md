# Easy Package

Easy Package 是一个只读的本机开发环境管理器 MVP。它使用 Tauri 2、React、TypeScript、Rust 和 SQLite，统一发现并展示 Homebrew、npm、pnpm、uv、pip，以及用户明确选择目录中的 JavaScript / Python 项目元数据。

## 当前边界

- 首发支持 macOS；其他平台返回受控的“不支持”状态。
- 桌面窗口最小宽度为 880px，不提供移动端 Web 适配。
- 只执行版本、列表、更新检查、缓存路径等只读命令。
- 不提供安装、升级、卸载、清理或任意 Shell 执行接口。
- 项目扫描只读取 manifest、锁文件和运行时声明，不解析完整依赖树。
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

## 结构

- `src/`：React 页面、组件、Tauri API 封装和前端测试。
- `src-tauri/src/adapters/`：包管理器发现、命令执行与输出解析。
- `src-tauri/src/scan/`：项目扫描、PATH 检查和健康规则。
- `src-tauri/src/storage.rs`：SQLite 快照、根目录和日志存储。
- `src-tauri/src/commands.rs`：对前端开放的七个受控 Tauri command。

命令执行统一使用可执行文件路径与参数数组，设置 20 秒超时，不经过 Shell。诊断输出会替换用户主目录并限制长度。
