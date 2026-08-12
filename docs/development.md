# 开发指南

面向贡献者的构建、验证与发布说明；产品使用说明见仓库 [README](../README.md)，行为边界规格见 [boundaries.md](boundaries.md)，协作约定见 [AGENTS.md](../AGENTS.md)。

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

`pnpm check` 会依次执行 Biome lint、前端测试、前端构建、Rust 单元测试与 Rust 格式检查；也可单独运行 `pnpm lint` / `pnpm lint:fix` / `pnpm format`。Node 版本以 `.nvmrc`（22）为准。

前端数据契约类型([src/types.gen.ts](src/types.gen.ts))由 Rust models 通过 specta 生成：`cargo test bindings` 会在类型漂移时失败（已包含在 `pnpm check` 与 CI 中）；修改 `models.rs` 后运行 `EASY_PACKAGE_EXPORT_TYPES=1 cargo test --manifest-path src-tauri/Cargo.toml bindings` 重新生成。`src/types.ts` 只保留 UI 专属类型。

后端使用 `tracing` 输出结构化日志：默认 `devpkg_lib=info`，可用环境变量 `EASY_PACKAGE_LOG` 调整（如 `EASY_PACKAGE_LOG=devpkg_lib=debug pnpm tauri dev` 可看到扫描阶段进度与外部命令 trace）。dev 构建为可读格式，release 构建为 JSON 行；日志不包含命令输出等已脱敏内容。
`pnpm build:desktop` 以 release 模式构建 Tauri 原生二进制，但通过 `--no-bundle` 保持不生成 `.app`、DMG 或安装包；产物位于已忽略的 `src-tauri/target/release/`。
GitHub Actions 会对 Pull Request 与 `develop` 推送执行质量门禁（`pnpm check`）。原生二进制构建（`pnpm build:desktop`）只在 `develop` 推送或手动触发 `quality.yml` 时运行，以缩短 PR 等待；发布打包仍由 `release.yml` 负责。

`pnpm test:e2e` 通过 `tauri-driver` 驱动真实 Tauri 窗口，并以 Rust `e2e` feature 和 `EASY_PACKAGE_E2E=1` 返回固定扫描 fixture。它不属于常规 CI 门禁：`tauri-driver` 不支持 macOS，且 Linux WebKit 驱动与固定 fixture 的维护成本不适合当前 macOS-first MVP。需要发布级原生验证时，在受控 Linux 环境或后续稳定的 macOS 原生方案中手动运行；该模式不读取本机包管理器、扫描目录或 SQLite。在 macOS 上以下方「桌面烟测」清单作为替代验证手段。

## 桌面烟测

原生构建或 `pnpm tauri dev` 成功，只证明应用可以构建和启动，不替代 GUI E2E 或真实使用验证。每次涉及扫描、依赖或界面改动时，在 1440px 窗口完成以下只读检查：

1. 使用无历史快照的测试配置启动，确认应用停在扫描启动器且不会自动扫描；点击「扫描」，确认扫描可以取消，再次完成扫描后进入概览，并验证软件包筛选与空态。
2. 在系统临时目录创建测试项目目录，添加后检查项目和依赖解析来源，再从扫描根目录移除。
3. 验证环境页的健康项与诊断日志。
4. 调整扫描范围后确认项目与依赖洞察同步更新；完成第二次扫描后，在“历史”页检查快照比较与筛选。
5. 导出一份环境报告和变化报告，检查主目录已脱敏且不含诊断原始输出。
6. 在“项目分析”页的“依赖图”视图按需解析项目图，检查直接/传递/重复版本筛选与依赖路径；切到“供应链风险”视图确认项目选择保持一致，检查结构性风险证据并导出一份 SBOM。
7. 在“操作中心”分别生成 Homebrew、npm 与 pnpm 的安装、升级、卸载和缓存维护计划，核对固定命令、Node/prefix 预检、确认门槛、恢复阻塞与取消提示；真实执行仅使用隔离验收环境。
8. 删除临时测试目录，确认不会遗留扫描根目录。

## 结构

- `src/`:React 页面、组件、Tauri API 封装和前端测试；`src/types.gen.ts` 由 Rust models 生成，勿手改。
- `src-tauri/src/adapters/`：包管理器发现、命令执行与输出解析。
- `src-tauri/src/scan/`：扫描编排、PATH 检查与健康规则。
  - `scan/projects/`：目录遍历、manifest 解析、锁文件关联、工作区与依赖索引。
  - `scan/dependency_graph/`：受预算限制的完整依赖图（构建 / 解析器 / SBOM)与 digest 缓存。
  - `scan/supply_chain.rs`：不联网的结构性供应链规则、稳定规则 ID、证据和依赖路径。
- `src-tauri/src/actions/`：受控写操作（计划 / 执行 / 固定参数 / 可执行文件验证 / 能力矩阵 / 复扫核对 / 目录搜索）。
- `src-tauri/src/services.rs`：命令层背后的业务编排（报告导出、快照比较、写操作流程）。
- `src-tauri/src/storage.rs`:SQLite 快照、根目录、扫描设置与日志存储，含 user_version 迁移框架。
- `src-tauri/src/commands.rs`：对前端开放的薄 command 层，只做参数校验与转发；写操作只能消费一次性计划。
- `src-tauri/src/error.rs`：结构化 AppError({ code, message }),code 是跨栈稳定契约。

命令执行统一使用可执行文件路径与参数数组，设置 20 秒超时，不经过 Shell。诊断输出会替换用户主目录并限制长度。
