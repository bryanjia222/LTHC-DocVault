# 产品规格

本文描述桌面应用的行为规格：对象何时可见、何时可用、点击后发生什么、状态如何切换、错误如何呈现，以及哪些结果会持久化。界面对象的名称和实现锚点见 [`docs/ui-terminology.md`](./ui-terminology.md)。正文中只引用术语 ID，例如 `toolbar.preview-button`，不重复展开中英文文案。

## 规格约定

- 本文描述生产路径和开发路径的公共行为；开发专用能力只在对应小节明确标注。
- 术语 ID 是稳定锚点，不随界面文案或组件改名变化。
- `term.selected-document` 表示用户在文档列表中显式选中的文档；`term.selected-version` 表示版本历史中显式选中的版本。
- 应用状态分为四类：
  - 仓库状态：后端数据库、配置、归档仓库、任务结果。
  - 桌面状态：项目、标签、源文件追踪、回收站隐藏状态、排序偏好。
  - 客户端偏好：主题、语言、列设置、详情面板固定状态等。
  - 运行时状态：选中对象、筛选条件、对话框、覆盖层、展开状态。
- `isTauri()` 为真时使用本地后端；浏览器开发模式使用 mock 数据。mock 只用于开发，不改变生产规格。
- 前端自身产生的错误通过共享错误上报器进入持久日志；后端命令失败已在 Rust 边界记录，不再从浏览器侧重复上报。
- 用户可见文案中的“对比”指生成对比结果；“红线”只允许作为历史实现描述，不作为产品用语。

## 启动与仓库连接

### 启动

应用进入 `app.shell` 后先显示 `boot.loading`。启动流程依次完成：

1. 连接或打开当前仓库。
2. 读取文档、项目、标签、源文件追踪、回收站状态和任务状态。
3. 加载桌面状态、客户端偏好、常用链接和预览缓存索引。
4. 启动源文件修改探测、任务事件订阅和亲笔信全局状态轮询。
5. 对可见文档执行一次预览缓存预热。

连接失败时显示 `boot.open-error`，并保留 `boot.connect-button` 进入连接流程。连接成功后进入 `documents.panel`。

首次进入文档视图时，应用自动选中第一个可见文档；之后不再自动覆盖用户显式做出的选择或清空操作。

### 仓库连接

`boot.connect-button`、`status.switch-button` 和引导入口都会打开 `connect.flow`。用户可选择 `connect.new-vault` 或 `connect.open-vault`：

- 新建仓库需要选择目录、选择后端、设置仓库密码并确认密码。
- 打开已有仓库需要选择目录和仓库密码；后端类型从现有配置读取，不能重新选择。
- 新建路径会初始化数据库、归档仓库和本地状态；打开路径只验证并加载已有仓库。
- 成功后刷新文档、项目、任务、仓库统计和亲笔信状态。
- 失败时保留对话框和用户输入，错误在对话框内呈现。

浏览器开发模式可以进入连接界面，但不要求真实仓库可用；Tauri 生产路径执行真实后端。

## 导航与选中语义

`nav.primary` 提供 `nav.documents`、`nav.trash`、`nav.settings` 和亲笔信入口。`term.active-section` 决定主视图；切换视图不销毁仓库数据。

选中语义遵循显式选择：

- 点击 `documents.document-row` 设置 `term.selected-document`。
- 点击 `detail.version-row` 设置 `term.selected-version`。
- 点击项目树、表格空白区、表头或其他非文档条目时清除 `term.selected-document` 和 `term.selected-version`。
- 切换到 `nav.trash` 或 `nav.settings` 时清除文档和版本选中状态。
- 用户显式清空选中后，应用不会在列表刷新、任务完成或窗口重新获得焦点时自动恢复选中。

顶部文档操作依赖 `term.selected-document`；版本详情操作依赖 `term.selected-document` 和 `term.selected-version`。回收站和设置视图中的按钮只作用于当前视图内明确选择的对象，不能复用文档视图遗留的选中状态。

## 项目树与组织

`term.project` 是桌面本地的组织单元。每个 `term.document` 只能属于一个 `term.project`；未分配文档归属根级“全部文档”视图可见。

### 显示与状态

- `sidebar.project-tree` 展示项目层级。
- `sidebar.project-row` 表示项目；展开状态只在当前会话内有效。
- 点击项目会切换 `term.active-project`，文档列表按该项目及其子项目过滤。
- 点击 `nav.all-documents` 显示全部文档。
- `documents.group-divider` 只在存在多个项目分组时显示。

### 项目创建与重命名

`sidebar.project-input` 和 `sidebar.context-menu` 都可创建项目：

- 新名称会去掉首尾空白。
- 同级项目名不区分大小写去重。
- 重名校验失败时在 `sidebar.project-input` 附近显示错误。
- 重命名保存为桌面状态。

### 项目删除

`sidebar.menu.delete-project` 先确认。删除规则：

- 子项目上移到被删项目的父级。
- 文档的项目归属被移除，文档本体不删除。
- 项目的排序偏好被移除。
- 当前项目被删除后，视图回到可见的项目或 `nav.all-documents`。

### 拖放

- 文档拖到 `sidebar.project-row` 上会改变文档的项目归属。
- 项目拖到另一个项目上会重新建立父子关系；祖先不能拖到自己的后代下，避免循环。
- 项目拖到 `nav.all-documents` 上会移到根级。
- 在不同项目间移动已追踪文档时先确认；确认后才提交桌面状态。
- 移动项目只影响组织结构，不移动磁盘文件。

## 文档列表、筛选与排序

### 搜索与类型筛选

- `documents.search-scope` 支持全部、标签、文件名、作者和文档 ID。
- `documents.search-input` 的匹配不区分大小写。
- `documents.filter-type` 提供文档、演示文稿、表格三类；`documents.filter.document`、`documents.filter.presentation`、`documents.filter.spreadsheet` 可组合选择。
- `documents.filter-count` 显示启用筛选的数量，`documents.filter-clear` 清除搜索与类型筛选。
- `documents.visible-count` 显示过滤后的文档数量。

### 作用域与默认排序

- `term.active-project` 非空时，列表显示该项目及其后代项目中的文档。
- `nav.all-documents` 显示全部非回收站文档。
- 默认按“更新时间”降序排序。
- 同一列再次点击切换升序和降序；切换到新列从升序开始。
- 排序偏好按项目作用域保存到桌面状态。

### 表格与行

- `documents.table` 的列宽和列可见性保存在客户端偏好中。
- `documents.column.name` 始终显示。
- `documents.column.owner` 与 `documents.column.status` 默认隐藏。
- `documents.col-resizer` 调整列宽，`documents.sort-indicator` 表示当前排序方向。
- `documents.file-type-badge` 显示扩展名类别；`documents.row-tags` 显示文档标签；`documents.status-pill` 和 `documents.modification-pill` 显示健康与源文件状态。

### 行内操作

`documents.row-actions` 默认隐藏，在行悬停或键盘聚焦时显示：

- `documents.row.open-button` 打开当前库副本。
- `documents.row.preview-button` 预览当前工作内容。
- `documents.row.commit-button` 只在 `state.modification.modified` 时可执行。
- `documents.row.export-button` 导出当前工作内容。

点击 `documents.non-document-area`、表头或分组线会清除选中状态；点击行内按钮不改变选中文档。

## 顶部工具栏与文档动作

`toolbar.actions` 中的五个核心动作必须满足选择语义：

| 控件                     | 可用条件                                                                 | 行为                             |
| ------------------------ | ------------------------------------------------------------------------ | -------------------------------- |
| `toolbar.preview-button` | 存在 `term.selected-document`                                            | 预览当前工作内容或显式选中的版本 |
| `toolbar.compare-button` | 存在 `term.selected-document`                                            | 打开对比选择对话框               |
| `toolbar.open-button`    | 存在 `term.selected-document`                                            | 打开库副本                       |
| `toolbar.export-button`  | 存在 `term.selected-document`                                            | 导出当前工作内容                 |
| `toolbar.commit-button`  | 存在 `term.selected-document` 且修改状态为 `state.modification.modified` | 提交已修改源文件                 |

没有选中文档时，这些按钮禁用；`toolbar.theme-toggle` 和 `toolbar.palette-button` 仍然可用。

### 双击

双击 `documents.document-row` 的动作由 `appearance.double-click` 决定：

- `appearance.double-click.preview` 执行预览。
- `appearance.double-click.open` 执行打开。

### 预览

预览优先使用当前工作内容；如果用户在版本历史中显式选择了历史版本，则预览该版本。预览打开 `preview.overlay`，具体规则见“预览与缓存”。

### 打开

打开动作的目标由来源决定：

- 打开当前内容时，打开可编辑的库副本。
- 打开历史版本时，先生成只读临时文件再打开。
- 双击配置为打开时，走“打开当前内容”路径。

### 提交修改

`toolbar.commit-button` 和 `dialog.commit-modified` 的流程：

1. 验证 `term.source-file` 存在且修改状态为 `state.modification.modified`。
2. 打开 `dialog.commit-modified`。
3. `dialog.commit-modified.doc` 只读，`dialog.commit-modified.note` 可选。
4. 提交创建 `job.kind.commit`，对话框关闭。
5. 任务成功后刷新文档与版本列表，更新修改状态、健康状态和更新时间。
6. 任务失败时通过 `toast.item` 报告，并保留可重试入口。

### 导出

导出当前工作内容是同步流程：

1. 显示原生保存对话框。
2. 用户选择目标路径。
3. 应用写入目标文件。
4. 成功后显示活动日志或通知；失败时报告错误并保留原文件。

导出历史版本走版本历史入口，导出的是该版本提交时的内容。

### 添加与新建

`documents.import-button`、`sidebar.menu.import-documents` 和窗口拖放都会打开 `dialog.add-document`。

支持受管理扩展名的文件。导入流程：

1. 选择或拖入文件后填充 `dialog.add-document.file-field`。
2. 一个文件显示单个表单；2-6 个文件显示 `dialog.add-document.import-card`；超过 6 个文件显示 `dialog.add-document.bulk-block`。
3. 用户可修改 `dialog.add-document.name-field` 和 `dialog.add-document.author-field`，可移除单个待导入文件。
4. `dialog.add-document.project-select` 决定导入后的项目归属。
5. 逐个文件提交；单个文件失败不会中断整批任务。
6. `dialog.add-document.progress` 显示当前进度。
7. 批量结束后刷新列表，识别新文档，建立库副本，开始源文件追踪，并应用项目归属。

新建文档由 `documents.new-document-button` 或 `sidebar.menu.new-file` 进入 `dialog.new-document`。支持 TXT、Markdown、DOCX、XLSX 和 PPTX；PPTX 可选择 16:9 或 4:3。创建成功后生成初始版本、库副本和源文件追踪。

### 替换提交

`menu.doc.replace-commit` 用于把一个新的本地文件替换为文档的下一个版本：

- 新文件扩展名必须与原文档相同。
- 如果当前源文件已被修改，先确认是否先提交现有修改。
- 现有修改提交失败时中止替换流程。
- 替换成功后重新建立源文件追踪并刷新列表。

### 重命名、备注与标签

- `dialog.rename` 修改文档显示名。名称会修剪；未变化时取消。
- `dialog.note-edit` 修改 `term.selected-version` 的备注。未变化时取消；空内容清除备注。
- 标签在详情和行内显示，按桌面状态持久化。输入会修剪和去重；`detail.tag-remove` 移除单个标签。

## 详情面板与版本历史

### 面板

- `detail.panel` 默认未固定。
- 未固定状态下，焦点离开文档详情时自动收起。
- `detail.pin-button` 可固定面板；固定状态保存为客户端偏好。
- 选中文档变化时，版本视图回到列表模式，`detail.maximized-overlay` 关闭。

### 版本列表

- `detail.version-history` 显示 `term.version` 集合。
- `detail.version-count` 显示总版本数。
- `detail.view-mode` 支持列表模式和版本树模式。
- 版本树模式只有在存在分支历史时显示；线性历史仍可使用列表模式。
- `detail.version-row` 显示状态、更新时间、备注和派生关系。
- `term.selected-version` 为空时，详情默认落到第一个可见的非回收站版本。
- `detail.based-on` 显示当前版本的来源版本。

### 版本树图

- `detail.version-graph` 支持平移和缩放视图。
- `detail.graph-reset` 重置视口。
- `detail.graph-maximize` 打开 `detail.maximized-overlay`。
- `detail.maximized-stage` 提供更大的图区，`detail.maximized-context` 显示选中版本信息。
- `detail.graph-minimize` 回到文档详情面板。

### 切换版本

`detail.checkout-button` 和 `menu.version.checkout` 都执行切换：

- 当前版本不允许切换到自身。
- 确认后创建 `job.kind.checkout`。
- 运行中任务将当前版本指针切换到目标版本，并重写库副本。
- 任务成功后重新基线化源文件追踪，清除源文件修改状态。
- 任务失败时保留原指针和原库副本。

## 右键菜单与全局菜单

所有瞬时右键菜单共享关闭规则：窗口失焦、点击菜单外、按 Escape、切换视图或组件卸载时关闭。右键菜单位置会按视口边界调整。

### 文档右键菜单

`menu.doc` 提供：

- `menu.doc.preview`、`menu.doc.open`、`menu.doc.export`
- `menu.doc.commit`、`menu.doc.replace-commit`
- `menu.doc.rename`
- `menu.doc.remove-from-project`
- `menu.doc.delete`
- `menu.doc.refresh`
- `menu.doc.properties`

规则：

- `menu.doc.commit` 只在 `state.modification.modified` 时可用。
- `menu.doc.remove-from-project` 只有在 `term.active-project` 非空时出现。
- `menu.doc.delete` 是可恢复的软删除，进入回收站。
- `menu.doc.properties` 打开 `dialog.document-status`。

### 版本右键菜单

`menu.version` 提供：

- `menu.version.preview`
- `menu.version.export`
- `menu.version.compare-latest`
- `menu.version.checkout`
- `menu.version.delete`
- `menu.version.refresh`

规则：

- `menu.version.compare-latest` 只对 DOCX 版本显示，且目标版本不是 `term.current-version`。
- `menu.version.delete` 会向下级联删除衍生版本；确认列表中列出所有会删除的版本。
- 当前版本出现在删除子树内时禁止删除。
- 删除子树等于完整历史时禁止删除。

### 其他菜单

- `menu.preview.reload` 重新加载预览；重新加载绕过内存缓存和磁盘缓存。
- `menu.app.refresh` 在任意区域可用。
- `menu.app.inspect` 只在开发构建且 `appearance.dev-mode` 打开时显示。

## 对话框

`dialog.overlay`、`dialog.panel`、`dialog.title`、`dialog.subtitle`、`dialog.close` 和 `dialog.footer` 构成模态外壳。确认类流程可能使用 `dialog.native-confirm`。

### 提交修改

见“提交修改”。关闭对话框不会取消已经在后端创建的任务；任务最终状态以后端事件为准。

### 重命名与备注

见“重命名、备注与标签”。同步完成后立即刷新界面状态；失败时保留输入并显示错误。

### 文档属性

`dialog.document-status` 是只读属性视图，但允许修改项目归属：

- `dialog.document-status.modification` 显示源文件修改状态。
- `dialog.document-status.source-path` 显示当前被追踪的源文件路径。
- `dialog.document-status.document-id` 显示稳定文档 ID。
- `dialog.document-status.backend` 显示备份后端。
- `dialog.document-status.project-select` 修改文档所属项目。

### 对比选择

`toolbar.compare-button` 和 `menu.version.compare-latest` 打开 `dialog.compare`：

- `dialog.compare.old-side` 与 `dialog.compare.new-side` 分别选择文档和版本。
- 两侧都只选择非回收站的已提交版本，不使用工作副本。
- 旧侧默认取当前选中文档和选中版本；新侧默认取目标文档的 `term.current-version`。
- 两侧必须是 DOCX。
- 选择相同文档和相同版本时显示 `dialog.compare.hint`，并禁止 `dialog.compare.run`。
- 点击 `dialog.compare.run` 后关闭选择对话框并打开 `compare.overlay`。

### 常用链接

`dialog.quick-link` 用于 `term.quick-link` 的新增与编辑：

- 输入裸域名时自动补全为 HTTPS。
- `dialog.quick-link.fetch` 尽力获取网页标题和站点图标。
- 自动填充只发生在名称为空时，不覆盖用户已输入的名称。
- 图标每次保存时刷新。
- 新增和编辑都写入客户端偏好。

## 命令面板

`toolbar.palette-button` 或 Ctrl/Cmd + K 打开 `palette.overlay`。打开时清空 `palette.input` 并自动聚焦。

命令分为两类：

- `palette.navigation-group` 提供文档、回收站、设置和状态页签的直达命令。
- `palette.action-group` 提供打开、提交、导出、切换版本、刷新和切换主题。

`palette.input` 按命令名称过滤，不区分大小写。列表为空时显示 `palette.empty`；有结果时按分组显示。键盘 Up/Down 循环选择，Enter 执行当前命令，Escape 关闭面板。命令动作执行后面板关闭。

命令面板中的文档动作复用顶部工具栏和详情面板的可用条件；没有满足选择语义或修改状态时，对应动作不得执行。

## 预览与缓存

### 支持范围

预览支持 PDF、Markdown、TXT、DOCX、XLSX、PPTX：

- DOCX、XLSX、PPTX 只有内容是 OOXML ZIP 时可预览。
- 旧版 Office 二进制和未知格式显示 `preview.unsupported`。

### 打开与状态

预览入口打开 `preview.overlay` 和 `preview.modal`：

- `preview.body` 显示渲染内容。
- 首次加载显示 `preview.loading`。
- 后端或渲染失败显示 `preview.error`。
- 不支持格式显示 `preview.unsupported`。
- `preview.close`、Escape 或覆盖层点击关闭预览。

### 缓存

预览与对比共享内存缓存和磁盘缓存：

1. 先查内存 LRU。
2. 未命中时查磁盘缓存。
3. 仍未命中时向仓库后端获取内容并渲染。
4. 渲染成功后写入内存 LRU 和磁盘缓存。

内存 LRU 容量为 24 条。缓存键由文档 ID 和版本标识组成：

- 历史版本使用 `docId|v:<label>`。
- 已修改工作副本使用 `docId|working:<current-label>`。
- 未修改当前版本使用 `docId|current:<current-label>`。

只有 `term.selected-version` 为空且修改状态为 `state.modification.modified` 的内容允许渲染可变预览。刷新可变预览在后台运行，渲染完成后原地替换，保留滚动位置并更新缓存。

同一个预览入口的旧加载请求会被新请求取代；旧请求完成后不得覆盖新请求的结果。

启动预热按最旧优先从磁盘缓存填充内存 LRU，不触发后端重新渲染。

## 对比

对比只处理已提交的 DOCX 版本，不读取工作副本。对比结果共享预览的内存 LRU 和磁盘缓存。

缓存键为 `compare|<oldDocId>:<oldLabel>|<newDocId>:<newLabel>`。

加载顺序：

1. 内存 LRU。
2. 磁盘缓存。
3. 后端获取旧版和新版内容。
4. Docxodus WASM worker 生成对比。
5. 结果写入缓存并显示。

生成期间显示 `compare.loading` 和持续旋转的加载指示，文案为“生成对比中”。失败时显示 `compare.error`。完成或失败都不会把中间结果写入长期缓存。

对比窗口通过 `compare.close`、Escape 或覆盖层点击关闭。Docxodus worker 是长驻单例，关闭对比窗口不销毁 worker。

## 回收站

`nav.trash` 打开 `trash.panel`。删除是桌面本地隐藏；仓库数据在后端删除操作后才会真正移除。

### 文档

文档软删除：

- 文档从文档列表隐藏。
- 桌面标签、项目归属和源文件追踪保留。
- `nav.trash-count` 更新。
- 恢复只是移除桌面隐藏状态，不调用后端。

永久删除文档：

1. 两次确认。
2. 创建 `job.kind.delete`。
3. 立即清除桌面标签、项目归属和源文件追踪。
4. 尽力移除本地库副本。
5. 不删除用户的原始源文件。

### 版本

版本软删除会级联到所有 `term.descendant-version`：

- 确认对话框列出受影响版本。
- 用户拒绝时整个级联删除取消。
- 删除子树包含任何 `term.current-version` 时禁止删除。
- 删除子树等于完整历史时禁止删除。

恢复版本会向上级联恢复所有被删除的 `term.ancestor-version`；用户拒绝时整个级联恢复取消。

永久删除版本只删除该版本和当前处于回收站中的衍生版本；仍可见的衍生版本保留。

### 清空回收站

`trash.empty-trash` 显示文档数和独立删除的版本数，然后两次确认。执行规则：

- 文档逐个永久删除。
- 版本删除按所属文档分组执行。
- 单项失败记录错误，不中断剩余项。
- 属于已删除文档的版本跳过单独删除，因为文档删除已包含其版本。

`trash.restore-button` 恢复软删除条目；`trash.delete-button` 永久删除条目。回收站为空时显示 `trash.empty-state`。

## 任务、通知与活动日志

### 任务

`term.job` 的最终状态由后端事件决定。`job:update` 更新任务状态、进度和错误信息。

- 运行中的任务计入任务数量。
- 取消请求只是请求；只有终端状态 `state.job.cancelled`、`state.job.succeeded` 或 `state.job.failed` 才是最终状态。
- `status.job-progress` 显示后端给出的进度；后端没有进度时显示不确定状态。
- `status.job-kind`、`status.job-target` 和 `status.job-status` 说明任务对象与状态。
- 运行中的任务提供 `status.job-cancel`。

成功任务会触发对应的界面刷新：

- 提交刷新文档和版本列表。
- 切换版本刷新文档、版本、源文件状态和库副本。
- 删除刷新回收站、列表和仓库统计。
- 压缩归档和创建空白文档刷新列表与仓库统计。

### 通知

- `toast.host` 最多显示 4 条 `toast.item`。
- 终端状态通知在 4500ms 后自动消失。
- 用户可使用 `toast.dismiss` 手动关闭。
- `toast.status` 显示 `state.job.running`、`state.job.succeeded`、`state.job.failed` 或 `state.job.cancelled`。

### 活动日志

- `status.activity-log` 保留最近 8 条 `status.log-entry`。
- `status.log-clear` 清空日志。
- 清空动作本身也会生成一条日志，避免界面看起来没有反馈。

## 设置

`nav.settings` 打开设置视图，`settings.tabs` 默认进入外观页签。

### 状态页签

`settings.tab.status` 显示：

- `status.vault-card` 与 `status.metrics`
- `status.switch-button`，打开 `connect.flow`
- `status.tasks-panel`
- `status.activity-log`
- `status.archive-panel` 和仓库目录、暂存目录、后端、密码状态、快照统计
- `status.database-card` 与 `status.database-path`
- `status.logging-card` 与 `status.log-level`、`status.log-file`
- `status.reload-app-card`

仓库大小请求会合并重复请求，并使用 10 秒 TTL。任务成功、连接切换或开发者重置后强制刷新。

仓库密码项显示是否已设置；规格不要求在状态页签中回显明文密码。

### 外观页签

`settings.tab.appearance` 包含：

- `appearance.theme-control` 支持 Light、System、Dark。
- System 跟随操作系统主题实时变化。
- `appearance.language` 切换界面语言；语言变化需要重启应用，因为编辑器组件在会话中绑定语言资源。
- `appearance.double-click.preview` 和 `appearance.double-click.open` 控制双击动作。
- `appearance.columns` 控制列可见性和宽度；`appearance.columns-always-on` 说明始终显示列；`appearance.columns-reset` 恢复列默认值。
- `appearance.reset-defaults` 恢复主题、双击、开发者模式、列设置和详情面板固定状态。

### 开发者专用

开发构建且 `appearance.dev-mode` 打开时显示 `dev.reset-card`：

- `dev.stage-slider` 和 `dev.stage-confirm` 用于开发数据重置。
- `dev.qinbixin-environment` 切换亲笔信测试站点。
- `dev.qinbixin-accounts` 快速切换测试账号。

这些能力用 `import.meta.env.DEV` 编译期隔离，不进入生产构建。

## 亲笔信

亲笔信是独立的信件功能，不影响文档仓库状态。

### 全局状态

`sidebar.qinbixin-row` 通过 `sidebar.qinbixin-state` 显示登录和未读状态。全局状态每 5 秒轮询一次。

### 登录

未登录时显示 `qinbixin.login-panel`：

- `qinbixin.account-field` 和 `qinbixin.password-field` 为必填。
- 登录失败在面板内显示错误。
- 授权过期会回到未登录状态。

### 信箱

登录后打开 `qinbixin.dialog`，可切换 `qinbixin.inbox`、`qinbixin.outbox` 和 `qinbixin.compose`。

信箱打开且已登录时，会话数据每 5 秒轮询一次。未读状态分为会话未读和评论未读；评论未读水位按环境和账号保存在客户端偏好中。

- `qinbixin.mark-all-read` 标记全部会话和评论已读。
- 选中会话会标记该会话已读。
- `qinbixin.reply` 进入回信视图。
- `qinbixin.logout` 清空本地会话并回到登录面板。

### 发信

`qinbixin.compose` 需要：

- `qinbixin.recipient-select`
- `qinbixin.title-field`
- `qinbixin.content-field`

富文本内容在发送前清洗。`qinbixin.attachments` 支持图片、视频和附件，上传进度逐项显示。发送成功后进入发信箱并刷新状态。

外部链接在系统浏览器中打开，不在内嵌 WebView 中导航主应用。

## 持久化模型

| 存储                | 内容                                                                             |
| ------------------- | -------------------------------------------------------------------------------- |
| 仓库后端            | 文档、版本、任务、仓库配置、归档仓库、预览磁盘缓存                               |
| 桌面状态 JSON       | 标签、源文件追踪、项目、文档项目归属、排序偏好、回收站隐藏状态                   |
| 客户端 localStorage | 主题、语言、开发者模式、双击动作、详情面板固定、表格列、常用链接、亲笔信评论水位 |
| 运行时内存          | 选中对象、筛选条件、项目展开状态、版本视图模式、图缩放、对话框和覆盖层           |

仓库配置只来自磁盘上的 `config.toml` 和调用方显式参数；应用不读取 `DOCVAULT_*` 环境变量。

## 错误与加载原则

- 每个异步入口都有加载状态；生成类长任务使用持续旋转指示，避免看起来卡死。
- 任何最终失败必须出现在持久日志，并给用户可理解的通知。
- 单项批量操作失败不中断其他项，除非失败导致后续依赖步骤不成立。
- 终端任务状态以后端事件为准，前端预测状态不用于覆盖最终结果。
- 对失败后的可重试入口保持可用；不能把一次失败固化为永久不可用。

## 不变量

以下行为在回归测试中应优先覆盖：

1. 无 `term.selected-document` 时，`toolbar.preview-button`、`toolbar.compare-button`、`toolbar.open-button`、`toolbar.export-button` 禁用。
2. `toolbar.commit-button` 只在 `state.modification.modified` 时可用。
3. 点击非文档条目或切换到回收站、设置时清除文档与版本选中。
4. 所有右键菜单在窗口失焦时关闭。
5. 对比两侧选择相同文档和相同版本时被禁止。
6. 对比加载期间显示“生成对比中”和旋转指示。
7. 预览和对比共享内存 LRU 与磁盘缓存。
8. 文档软删除可恢复，永久删除必须两次确认。
9. 版本删除和恢复的级联确认被拒绝时完全取消。
10. 后端任务失败不会破坏已有仓库数据或用户原始源文件。
