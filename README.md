# DocVault

## 项目简介

DocVault 是一个面向 Office / WPS 文档的本地优先版本化归档系统，用于管理 `.docx / .xlsx / .pptx` 等文档的历史演进版本。

系统通过对 OOXML 文件进行结构化展开，并结合可切换的本地备份后端，实现稳定、可恢复、可追溯的文档版本管理能力。

产品面向个人与团队的本地文档管理场景，并为未来云端协作与托管能力预留演进空间。

---

## 功能说明

### 文档管理
- 文档版本提交与注册
- 文档列表查看
- 文档基础信息展示
- UUID 文档定位与同名文档歧义处理

### 版本管理
- 自动生成版本记录
- 版本历史查看
- 版本备注与修改人记录
- 当前版本指针切换
- 任意版本导出

### 归档存储
- 可切换备份后端（`local-copy` / `restic`）
- 原始文档完整保存
- 版本级快照管理

### OOXML 处理
- Office 文件结构解析（ZIP 解压）
- 文件结构展开
- Restic 后端按解包后的 OOXML 目录进行备份
- 恢复时重新压缩为 `.docx / .xlsx / .pptx`

### 任务系统
- 提交与恢复任务异步执行
- 任务进度追踪
- 任务状态持久化

---

## 目录架构

本项目采用 monorepo 结构：

```text
docvault/
│
├── crates/
│   ├── core/              # 核心业务逻辑（文档/版本/任务）
│   ├── storage/           # 本地存储与归档适配层
│   ├── ooxml/             # OOXML 解压与结构处理
│   ├── jobs/              # 任务系统（提交/恢复/校验）
│
├── apps/
│   ├── cli/               # 命令行工具
│   ├── desktop/           # Tauri 桌面应用
│
├── shared/
│   ├── types/             # 通用数据结构与模型
│
├── third_party/
│   ├── restic/            # 随应用打包的 Restic 二进制与校验信息
│
├── docs/
├── Cargo.toml             # workspace 入口
```

### 备份后端

当前实现支持两个本地备份后端：

- `restic`：默认后端。提交版本时先解压 OOXML 包，把解包后的 `package/` 目录交给 Restic 备份；恢复时从 Restic snapshot 还原 `package/` 目录，再重新压缩为 Office 文件。
- `local-copy`：可选后端。提交版本时复制原始 Office 文件，恢复时从版本副本复制到输出路径。适合开发、测试和排查问题。

`local-copy` 不提供内容去重、快照仓库校验或 Restic 的压缩能力。`restic` 后端负责版本级快照和内容去重，但仍通过 storage 层暴露同一套 CLI/core 行为。

DocVault 主数据库不保存本地文件路径。提交时传入的路径只用于读取当前文件内容；持久化元数据只保留文档 ID、显示名称、版本关系、归档引用和 `original_filename` 等跨设备仍然有意义的信息。

### 第三方运行时资产

Restic 作为可选本地归档运行时，不应直接放在项目根目录。推荐按版本和目标平台存放：

```text
third_party/
  restic/
    0.19.1/
      manifest.toml
      checksums.txt
      licenses/
        LICENSE
      x86_64-pc-windows-msvc/
        restic.exe
      x86_64-unknown-linux-gnu/
        restic
      aarch64-unknown-linux-gnu/
        restic
      x86_64-apple-darwin/
        restic
      aarch64-apple-darwin/
        restic
```

发布桌面端时，可由构建脚本将对应平台的二进制复制到 `apps/desktop/src-tauri/binaries/` 或 Tauri 要求的 sidecar 目录。CLI 发布也应从同一份 `third_party/restic` 资产中选择目标平台文件，避免不同入口使用不同 Restic 版本。

开发和运行时查找 Restic 的优先级为：

1. 配置文件中的 `restic_path`
2. 环境变量 `DOCVAULT_RESTIC_PATH`
3. 应用打包内置的 Restic sidecar
4. 开发环境中的 `third_party/restic/<version>/<target>/restic(.exe)`
5. 系统 `PATH` 中的 `restic`

------

## 安装步骤

### 环境要求

- Rust 工具链（stable）
- Node.js（用于桌面端前端）
- Tauri 运行环境（桌面应用）

### 构建项目

```bash
git clone <repo_url>
cd docvault
cargo build
```

### 安装 CLI（可选）

```bash
cargo install --path apps/cli
```

### 测试与覆盖率

```bash
cargo test
cargo clippy --all-targets --all-features
cargo llvm-cov --workspace --summary-only
```

如果本机尚未安装覆盖率工具：

```bash
cargo install cargo-llvm-cov --locked
```

------

## 快速开始

### 1. 初始化环境

```bash
docvault init
```

### 2. 提交文档版本

```bash
docvault commit ./report.docx --name "report" --author "Bryan" --note "Initial commit"
```

### 3. 查看文档列表

```bash
docvault list --format table
```

### 4. 查看版本历史

```bash
docvault versions report --format table
```

`list`、`versions`、`current` 和 `config show` 支持 `--format table|json`。默认是 `table`，适合人工查看；`json` 适合脚本或前端调用，避免文件名、备注等字段中的空格影响解析。

### 5. 查看当前版本指针

```bash
docvault current report --format table
```

### 6. 导出版本

```bash
docvault export report --version v2 --output ./exports/
```

`export` 只写出文件，不改变 DocVault 内部的当前版本指针。

### 7. 切换当前版本

```bash
docvault checkout report --version v1
docvault checkout report --version v1 --output ./exports/
```

`checkout` 会把指定版本设为文档的当前版本；带 `--output` 时会同时导出该版本。

### 8. 查看有效配置

```bash
docvault config show --format table
```

如果存在多个同名文档，使用 `--id` 或 `name@id-prefix` 精确定位：

```bash
docvault versions report@550e8400 --format table
docvault export --id 550e8400 --version v2 --output ./exports/
```

------

## 使用示例

### 文档版本提交流程

```bash
docvault commit ./contract.docx --name contract --author "Bryan" --note "Updated signature page"
```

系统将执行以下流程：

1. 校验 Office 文件类型
2. 创建或定位文档记录
3. 记录版本元数据（author / note）
4. 按当前备份后端执行归档
5. 返回版本 ID

`local-copy` 后端会复制原始文件。`restic` 后端会先解压 OOXML 文件结构，再备份解包后的目录。

------

### 文档定位

DocVault 允许多个文档使用相同的显示名称。每个文档都会分配一个 UUID 作为稳定 ID。

支持三种定位方式：

- `name`：按显示名称查找；如果匹配多个文档会报错。
- `name@id-prefix`：人用精确定位，例如 `report@550e8400`。
- `--id <id-prefix>`：程序或脚本推荐使用。

### 版本导出与 Checkout

DocVault 区分两个版本选择词：

- `latest`：最高版本号。即使 checkout 到旧版本，`latest` 仍然指向编号最大的版本。
- `current`：当前版本指针。默认每次 commit 后指向新版本，checkout 后会指向被 checkout 的版本。

`export` 只导出某个版本的文件，不改变文档当前指针：

```bash
docvault export contract --version latest --output ./output
docvault export contract --version current --output ./output
```

`checkout` 会把指定版本设为当前版本；如果提供 `--output`，也会同时导出文件：

```bash
docvault checkout contract --version v1
docvault checkout contract --version v1 --output ./output
```

`current` 可查看当前版本：

```bash
docvault current contract --format table
```

导出流程将：

1. 定位版本信息
2. 按版本记录选择对应备份后端
3. 从版本副本或 Restic snapshot 还原内容
4. 对 Restic 后端恢复出的 OOXML 目录重新压缩
5. 输出 Office 文件

Checkout 额外会更新 `documents.current_version_id`。后续提交新版本时，新版本的 `parent_version_id` 会指向 checkout 后的当前版本。

### OOXML Manifest

每次 commit 会为 Office 包生成版本 manifest，并随版本记录持久化。当前 manifest 字段为：

| 字段 | 说明 |
| ---- | ---- |
| path | 包内相对路径 |
| size | entry 字节大小 |
| sha256 | entry 内容 SHA-256 |
| content_type | 可选内容类型；当前 MVP 暂为空 |

`versions --format json` 会返回 manifest，便于 CLI 脚本和后续 GUI 展示版本内文件清单。

------

## 配置说明（基础层）

系统基础配置存储于本地配置文件。默认位置由系统配置目录决定，也可以用 `DOCVAULT_ROOT_DIR` 覆盖：

```text
Windows: %APPDATA%/DocVault/config.toml 或 %LOCALAPPDATA% 对应的应用配置目录
macOS: ~/Library/Application Support/com.LTHC.DocVault/config.toml
Linux: ~/.config/docvault/config.toml
```

### 示例配置

```toml
[storage]
backend = "restic"
data_dir = "C:/Users/<user>/AppData/Roaming/DocVault/data"
repo_dir = "C:/Users/<user>/AppData/Roaming/DocVault/repo"
restic_password = "docvault-local-development-password"

[database]
path = "C:/Users/<user>/AppData/Roaming/DocVault/db.sqlite"

[logging]
level = "info"
```

------

### 配置项说明

#### storage

| 字段     | 说明                                 |
| -------- | ------------------------------------ |
| backend  | 当前备份后端：`local-copy` 或 `restic` |
| data_dir | 临时文件与 staging 目录              |
| repo_dir | 归档仓库存储路径                     |
| restic_path | 可选 Restic 可执行文件路径；为空时使用内置或自动发现 |
| restic_password | 本地 Restic 仓库密码；也可用 `DOCVAULT_RESTIC_PASSWORD` 覆盖 |

#### database

| 字段 | 说明                  |
| ---- | --------------------- |
| path | SQLite 数据库文件路径 |

#### logging

| 字段  | 说明             |
| ----- | ---------------- |
| level | 日志级别         |
| file  | 日志输出文件路径 |

------

## 实现说明（TODO）

以下能力在后续版本中逐步完善：

- 任务系统调度优化
- 归档性能优化策略
- 云端同步能力
- 多设备访问支持
- 权限与协作机制
- 插件扩展机制（未来版本评估）

------

## 设计原则

系统设计遵循以下原则：

- 核心逻辑与界面层分离
- 文档版本具有确定性与可恢复性
- 存储结构保持稳定与可追溯
- 系统行为具备可测试性与可重复性

------

## 技术栈

- Rust（核心逻辑）
- Tauri 2（桌面端）
- React / Vue + TypeScript（UI）
- SQLite（本地元数据）
- Restic（归档存储）
- Tokio（异步任务运行时）

------

## 未来演进方向（TODO）

- 云端服务支持
- 多端同步能力
- 协作与权限系统
- 存储层扩展能力
- 企业级部署模式

