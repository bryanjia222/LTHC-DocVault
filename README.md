# DocVault

## 项目简介

DocVault 是一个面向 Office / WPS 文档的本地优先版本化归档系统，用于管理 `.docx / .xlsx / .pptx` 等文档的历史演进版本。

系统通过对 OOXML 文件进行结构化展开与哈希分析，并结合高效的内容去重归档机制，实现稳定、可恢复、可追溯的文档版本管理能力。

产品面向个人与团队的本地文档管理场景，并为未来云端协作与托管能力预留演进空间。

---

## 功能说明

### 文档管理
- 文档导入与注册
- 文档列表查看
- 文档基础信息展示

### 版本管理
- 自动生成版本记录
- 版本历史查看
- 任意版本恢复

### 归档存储
- 基于内容去重的归档机制
- 原始文档完整保存
- 版本级快照管理

### OOXML 处理
- Office 文件结构解析（ZIP 解压）
- 文件结构展开
- 内容哈希生成与记录

### 任务系统
- 导入与恢复任务异步执行
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
│   ├── jobs/              # 任务系统（导入/恢复/校验）
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

### 第三方运行时资产

Restic 作为 v1 固定使用的本地归档运行时，不应直接放在项目根目录。推荐按版本和目标平台存放：

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

开发和运行时查找 Restic 的优先级建议为：

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

------

## 快速开始

### 1. 初始化环境

```bash
docvault init
```

### 2. 导入文档

```bash
docvault import ./report.docx --name "report"
```

### 3. 查看文档列表

```bash
docvault list
```

### 4. 查看版本历史

```bash
docvault versions report
```

### 5. 恢复版本

```bash
docvault restore report --version v2 --output ./restore/
```

------

## 使用示例

### 文档导入流程

```bash
docvault import ./contract.docx --name contract
```

系统将执行以下流程：

1. 复制原始文件至本地存储区域
2. 解压 OOXML 文件结构
3. 生成文件结构与哈希记录
4. 创建版本记录
5. 执行归档存储
6. 返回版本 ID

------

### 版本恢复流程

```bash
docvault restore contract --version latest --output ./output
```

恢复流程将：

1. 定位版本信息
2. 获取对应归档快照
3. 还原文件至临时目录
4. 输出原始 Office 文件

------

## 配置说明（基础层）

系统基础配置存储于本地配置文件：

```text
~/.docvault/config.toml
```

### 示例配置

```toml
[storage]
backend = "restic"
data_dir = "~/.docvault/data"
repo_dir = "~/.docvault/repo"
restic_path = ""

[database]
path = "~/.docvault/db.sqlite"

[logging]
level = "info"
file = "~/.docvault/logs/app.log"
```

------

### 配置项说明

#### storage

| 字段     | 说明                                 |
| -------- | ------------------------------------ |
| backend  | 当前归档后端实现（v1 固定为 restic） |
| data_dir | 临时文件与 staging 目录              |
| repo_dir | 归档仓库存储路径                     |
| restic_path | 可选 Restic 可执行文件路径；为空时使用内置或自动发现 |

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

