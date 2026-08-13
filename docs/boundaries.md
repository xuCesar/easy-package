# 行为边界规格

本文档是 Easy Package 各能力的精确行为边界，作为实现与验收的单一事实来源；产品化的能力介绍见仓库 [README](../README.md)。

## 边界清单

- 首发支持 macOS；其他平台返回受控的“不支持”状态。
- 桌面窗口最小宽度为 880px，不提供移动端 Web 适配。
- 只执行版本、列表、更新检查、缓存路径等只读命令；默认离线，只有用户将联网策略切换为“允许 registry 检查”后才执行更新检查。
- 包管理器命令按系统、已识别管理器目录和已识别用户工具目录分类；未知 PATH 可执行文件只展示，不执行。
- 扫描按管理器、项目、运行时、健康报告阶段显示进度，可随时取消；取消不会覆盖上一次成功快照。
- 同一时间只允许一个环境扫描；缓存目录统计支持取消，并在 3 秒或 10 万条目预算后明确标记为“部分”。
- 最近 10 份成功扫描快照可在“历史”页比较，按包管理器、软件包、项目和健康提示展示新增、移除与变化。
- 项目扫描可设置最大遍历深度与用户忽略目录；默认跳过 `node_modules`、`.git`、`target`、`dist`、`build`、`.venv` 与 `vendor`，并在扫描日志中说明跳过原因。
- 可通过系统保存对话框导出当前环境快照或快照变化为 JSON / Markdown；报告会将用户主目录替换为 `~`，且不包含诊断原始输出。
- 环境页会解析 node、npm、pnpm、Python/pip、Ruby/gem、PHP、Composer 的 PATH 优先级与候选来源，并只读标记命令冲突、运行时路径不一致和重复 Node 全局工具。
- 本机“运行时”页面只读发现 Node.js、Python 与 Rust 的当前及本地安装，识别 nvm、fnm、Volta、asdf、mise、pyenv、uv 与 rustup 来源；项目运行时声明与匹配结果在对应项目详情中展示，复杂版本范围只展示，不推测兼容性。
- “操作中心”支持 Homebrew Formula、npm 与 pnpm 全局包的安装、单个或批量升级、卸载和缓存维护：后端先生成一次性操作计划，展示固定命令、联网需求和风险，经用户二次确认后才执行。
- 安装模式可在用户主动将联网策略设为“允许 registry 检查”后，使用受信任的 Homebrew、npm 或 pnpm 可执行文件搜索软件包目录。搜索固定限制 20 条、20 秒超时、支持取消，并仅在内存中缓存 5 分钟；搜索结果只会填入安装目标，绝不会直接安装或写入 SQLite。
- Homebrew 写操作仅允许 `/opt/homebrew/bin/brew` 或 `/usr/local/bin/brew`；npm/pnpm 仅允许扫描得到且位于受管理或已识别用户工具目录中的可执行文件。计划会记录可执行文件指纹，执行前再次验证路径、大小和修改时间。
- npm 写操作还要求同目录 Node.js 与 PATH 当前 Node.js 一致，预检并锁定 global prefix 和 cache 路径；任一执行上下文在确认后变化都会拒绝操作。
- pnpm 安装只接受普通包名或 `@scope/name`，拒绝版本表达式、URL、Git 与本地路径来源；安装、升级和卸载固定带 `--ignore-scripts`，缓存清理仅调用 `pnpm store prune`。
- npm 使用相同包名边界，安装、升级和卸载固定带 `--ignore-scripts`；缓存维护仅调用 `npm cache verify`，不提供 `npm cache clean --force`。npm/pnpm 均不允许修改管理器自身。
- 写操作与环境扫描互斥，执行日志会脱敏并保存在本机审计记录中；取消或超时不会声称回滚，而是标记“状态未知”并强制重新扫描。
- 写操作开始前会先保存 `running` 审计；异常退出后新写操作保持阻塞，直到用户完成一次成功环境扫描并将遗留记录归档为“状态未知”。
- 操作中心会在生成计划前展示 Homebrew、npm、pnpm 各动作的类型化能力检查和稳定阻塞码；PATH、信任、恢复或数据路径不满足时直接禁用计划入口。
- 审计分别记录命令状态与复扫观察结果。安装、卸载、升级和缓存维护会依据前后快照标记 `applied`、`notApplied` 或 `ambiguous`；中断记录可通过“重新扫描并核对”解除阻塞，但不会伪造回滚或成功结论。
- Yarn、Bun、Cargo、uv、pip、RubyGems、Composer 等其他管理器仍保持只读；不提供 lifecycle scripts、任意 Shell、自动修复、后台升级或定时写操作。
- Yarn 仅支持 Classic 全局包目录扫描；Yarn Berry 会显示为已发现，但不扫描全局包。
- 项目扫描会从 Yarn Classic、Yarn Berry 与文本 `bun.lock` 关联 JavaScript 直接依赖的锁定版本；`bun.lockb` 仅展示受控限制提示，不尝试解析二进制内容。
- RubyGems 只读取本机的全局 gem 列表，不检查更新或执行写操作。
- Composer 只读取 Composer Home 中的 `vendor/composer/installed.json` 全局元数据；缓存目录通过受控的只读配置查询取得，不执行 `composer global show` 或更新检查。
- 项目扫描只读取 manifest、锁文件和运行时声明；npm、pnpm 与 Cargo 项目会生成完整依赖图摘要，完整节点与边仅在用户从项目详情打开完整依赖图或锁文件问题时按需重建，不写入 SQLite。
- “依赖”页面索引 JavaScript、Python、Rust、Go、Ruby 与 PHP 的直接声明依赖；跨生态同名包不会合并，并会标记跨项目的版本范围分歧。
- 对 package-lock、pnpm-lock、yarn.lock、bun.lock、Cargo.lock、uv.lock、Gemfile.lock 与 composer.lock，应用会只读关联直接依赖的已解析版本；无法匹配时明确显示“未解析”，不推测版本。
- 项目扫描支持 Poetry、Pipenv 与 Go modules；Poetry/Pipenv 分别读取其锁文件，Go 以 go.mod 的模块选择版本作为已解析版本来源，不执行模块下载。
- 项目扫描支持 Ruby 的 Gemfile/Gemfile.lock 与 PHP 的 composer.json/composer.lock；仅解析直接声明及锁定版本，不执行 Ruby DSL、Bundler 或 Composer 命令。
- 项目扫描会识别 JavaScript 与 Cargo 工作区，并提示工作区内版本分歧、未声明版本及本地依赖引用；Yarn、Bun、Python、Go、Ruby 与 PHP 暂不生成完整依赖图。
- 完整依赖图首批支持 `package-lock.json`、`pnpm-lock.yaml` 和 `Cargo.lock`，单锁文件限制 25 MB，单图限制 50,000 节点与 200,000 条边；超限会返回明确的“部分”状态，不静默截断结论。
- 项目详情的“锁文件问题”基于完整依赖图离线识别锁文件缺失或无效、同名多版本、本地/工作区引用、来源或版本缺失、不可达条目与循环回边，并展示本机证据和最短依赖路径。
- 可通过系统保存对话框导出 CycloneDX 1.6 SBOM；导出内容不包含本机路径，并只附带离线结构性风险摘要，不查询或伪造漏洞、许可证与修复版本。
- 扫描快照、扫描根目录、原始本机路径和诊断日志只保存在本机 SQLite 中；导出报告会脱敏主目录路径。
