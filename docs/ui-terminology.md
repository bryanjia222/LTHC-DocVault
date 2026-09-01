# UI 术语表

本表给界面上可见的内容与控件规定统一名称，供问题讨论、规格文档和实现绑定使用。行为、状态切换条件和触发流程写在产品规格中；本表只回答“这个东西叫什么、在哪里、绑定到哪些实现”。

讨论界面问题时，请引用术语 ID，例如“`toolbar.compare-button` 在无选中文档时应禁用”。不要用“上面那个按钮”“右侧面板里的东西”这类定位词。

## 结构与命名规则

术语 ID 采用 `<区域>.<对象>` 的稳定命名，与代码重构无关：

- ID 是永久锚点：文案改名不改 ID；只有对象本身的语义变化时才允许改 ID。
- 控件按类型加后缀：`-button`、`-input`、`-select`、`-menu`、`-panel`、`-card`、`-overlay`、`-row`、`-field`。
- 名称分中英文两列，必须与当前 i18n 文案一致。新增或修改文案时，应同步更新对应行。
- “绑定”列记录主要组件和代表性 i18n key。一个术语可能对应多个组件或 key；保持最新绑定即可，不要求穷尽。
- 本表不记录控件何时显示、何时可用、点击后发生什么。这些内容属于产品规格，规格引用这里的 ID。
- 同一个概念在正文、issue、测试中只能使用规范名称；不推荐用语见文末。

层级顺序为：应用 > 页面/视图 > 面板/区域 > 控件。描述位置时按这个顺序说，例如“文档列表 > 行内操作区 > 提交修改按钮”。

## 全局应用与导航

| 术语 ID           | 中文名称   | 英文名称           | 绑定（组件 / i18n key）                |
| ----------------- | ---------- | ------------------ | -------------------------------------- |
| app.shell         | 文档工作台 | Document Workspace | `App.vue` / `page.title`               |
| app.workspace     | 工作区     | Workspace          | `App.vue` `.workspace`                 |
| app.view-host     | 视图容器   | View host          | `App.vue` `.view-host`                 |
| nav.primary       | 主导航     | Primary            | `AppSidebar.vue` / `nav.primary`       |
| nav.documents     | 文档       | Documents          | `nav.documents`                        |
| nav.all-documents | 全部文档   | All documents      | `ProjectTree.vue` / `nav.allDocuments` |
| nav.settings      | 设置       | Settings           | `AppSidebar.vue` / `nav.settings`      |
| nav.trash         | 回收站     | Recycle bin        | `AppSidebar.vue` / `nav.trash`         |
| nav.trash-count   | 回收站计数 | Recycle-bin count  | `AppSidebar.vue` `.nav-badge`          |

## 侧边栏

| 术语 ID                       | 中文名称       | 英文名称              | 绑定（组件 / i18n key）                                          |
| ----------------------------- | -------------- | --------------------- | ---------------------------------------------------------------- |
| sidebar.brand                 | 品牌区         | Brand area            | `AppSidebar.vue` `.brand`                                        |
| sidebar.qinbixin-row          | 亲笔信导航行   | Qinbixin nav row      | `QinbixinNavRow.vue` / `qinbixin.title`                          |
| sidebar.qinbixin-state        | 亲笔信状态     | Qinbixin status       | `QinbixinNavRow.vue` / `qinbixin.loggedIn`, `qinbixin.loggedOut` |
| sidebar.quick-links           | 常用链接区     | Quick links           | `QuickLinksSection.vue` / `quickLinks.title`                     |
| sidebar.quick-link-row        | 常用链接行     | Quick-link row        | `QuickLinksSection.vue` `.quick-link-row`                        |
| sidebar.quick-link-more       | 链接更多操作   | Link more actions     | `QuickLinksSection.vue` / `sidebar.moreActions`                  |
| sidebar.project-tree          | 项目树         | Project tree          | `ProjectTree.vue`                                                |
| sidebar.project-row           | 项目行         | Project row           | `ProjectTree.vue` `.project-row`                                 |
| sidebar.project-expand        | 项目展开按钮   | Project expand button | `ProjectTree.vue` / `sidebar.toggleExpand`                       |
| sidebar.project-more          | 项目更多操作   | Project more actions  | `ProjectTree.vue` / `sidebar.moreActions`                        |
| sidebar.project-input         | 项目名称输入框 | Project name input    | `ProjectTree.vue` / `sidebar.projectPlaceholder`                 |
| sidebar.project-drop-hint     | 项目拖放提示   | Project drop hint     | `ProjectTree.vue` / `sidebar.projectDropHint`                    |
| sidebar.trash-row             | 回收站导航行   | Trash nav row         | `AppSidebar.vue` `.trash-row`                                    |
| sidebar.settings-row          | 设置导航行     | Settings nav row      | `AppSidebar.vue` `.settings-row`                                 |
| sidebar.context-menu          | 侧边栏右键菜单 | Sidebar context menu  | `SidebarContextMenu.vue`                                         |
| sidebar.menu.add-project      | 新建项目       | New project           | `SidebarContextMenu.vue` / `sidebar.addProject`                  |
| sidebar.menu.add-sub-project  | 新建子项目     | New sub-project       | `SidebarContextMenu.vue` / `sidebar.addSubProject`               |
| sidebar.menu.new-file         | 新建文档       | New document          | `SidebarContextMenu.vue` / `sidebar.newFile`                     |
| sidebar.menu.import-documents | 添加文档       | Add document          | `SidebarContextMenu.vue` / `sidebar.importDocument`              |
| sidebar.menu.expand-all       | 展开全部       | Expand all            | `SidebarContextMenu.vue` / `sidebar.expandAll`                   |
| sidebar.menu.collapse-all     | 折叠全部       | Collapse all          | `SidebarContextMenu.vue` / `sidebar.collapseAll`                 |
| sidebar.menu.rename-project   | 重命名         | Rename                | `SidebarContextMenu.vue` / `sidebar.renameProject`               |
| sidebar.menu.delete-project   | 删除项目       | Delete project        | `SidebarContextMenu.vue` / `sidebar.deleteProject`               |
| sidebar.menu.open-link        | 打开           | Open                  | `SidebarContextMenu.vue` / `quickLinks.open`                     |
| sidebar.menu.edit-link        | 编辑           | Edit                  | `SidebarContextMenu.vue` / `quickLinks.edit`                     |
| sidebar.menu.delete-link      | 删除           | Delete                | `SidebarContextMenu.vue` / `quickLinks.delete`                   |

## 顶部工具栏

| 术语 ID                | 中文名称   | 英文名称         | 绑定（组件 / i18n key）                     |
| ---------------------- | ---------- | ---------------- | ------------------------------------------- |
| toolbar                | 顶部工具栏 | App toolbar      | `AppToolbar.vue`                            |
| toolbar.actions        | 文档操作区 | Document actions | `AppToolbar.vue` `.toolbar-actions`         |
| toolbar.preview-button | 预览       | Preview          | `AppToolbar.vue` / `actions.preview`        |
| toolbar.compare-button | 对比       | Compare          | `AppToolbar.vue` / `actions.compare`        |
| toolbar.open-button    | 打开       | Open             | `AppToolbar.vue` / `actions.open`           |
| toolbar.commit-button  | 提交修改   | Commit modified  | `AppToolbar.vue` / `actions.commitVersion`  |
| toolbar.export-button  | 导出       | Export           | `AppToolbar.vue` / `actions.export`         |
| toolbar.theme-toggle   | 主题切换   | Theme toggle     | `AppToolbar.vue` / `actions.toggleTheme`    |
| toolbar.palette-button | 命令面板   | Command palette  | `AppToolbar.vue` / `actions.commandPalette` |

## 文档列表视图

| 术语 ID                          | 中文名称       | 英文名称                | 绑定（组件 / i18n key）                                |
| -------------------------------- | -------------- | ----------------------- | ------------------------------------------------------ |
| documents.panel                  | 文档面板       | Document panel          | `DocumentsView.vue` `.document-panel`                  |
| documents.header                 | 文档标题区     | Documents header        | `DocumentFilters.vue` `.panel-header`                  |
| documents.visible-count          | 可见文档数     | Visible count           | `DocumentFilters.vue` / `documents.visible`            |
| documents.search-scope           | 搜索范围       | Search scope            | `DocumentFilters.vue` / `search.scopeLabel`            |
| documents.search-input           | 搜索框         | Search input            | `DocumentFilters.vue` / `documents.searchPlaceholder`  |
| documents.filter-bar             | 筛选栏         | Filter bar              | `DocumentFilters.vue` `.filter-bar`                    |
| documents.filter-type            | 类型筛选       | Type filters            | `DocumentFilters.vue` / `filters.type`                 |
| documents.filter.document        | 文档类型筛选   | Document filter chip    | `filters.category.document`                            |
| documents.filter.presentation    | PPT 筛选       | Slides filter chip      | `filters.category.presentation`                        |
| documents.filter.spreadsheet     | 表格筛选       | Spreadsheet filter chip | `filters.category.spreadsheet`                         |
| documents.filter-count           | 筛选计数       | Active filter count     | `DocumentFilters.vue` / `filters.active`               |
| documents.filter-clear           | 清除筛选       | Clear filters           | `DocumentFilters.vue` / `filters.clear`                |
| documents.new-document-button    | 新建文档       | New document            | `DocumentFilters.vue` / `actions.newDocument`          |
| documents.import-button          | 添加文档       | Add document            | `DocumentFilters.vue` / `actions.importDocument`       |
| documents.table                  | 文档表格       | Document table          | `DocumentTable.vue`                                    |
| documents.group-divider          | 项目分组分隔线 | Project group divider   | `DocumentTable.vue` `.group-divider`                   |
| documents.non-document-area      | 非文档区域     | Non-document area       | `DocumentTable.vue` `.table-wrap`                      |
| documents.empty-state            | 空状态         | Empty state             | `DocumentTable.vue` `.empty-state` / `documents.empty` |
| documents.document-row           | 文档行         | Document row            | `DocumentRow.vue`                                      |
| documents.column.name            | 名称列         | Name column             | `documents.columns.name`                               |
| documents.column.owner           | 作者列         | Author column           | `documents.columns.owner`                              |
| documents.column.current-version | 当前版本列     | Current-version column  | `documents.columns.currentVersion`                     |
| documents.column.status          | 状态列         | Status column           | `documents.columns.status`                             |
| documents.column.modification    | 源文件列       | Source column           | `documents.columns.modification`                       |
| documents.column.updated         | 更新时间列     | Updated column          | `documents.columns.updated`                            |
| documents.file-type-badge        | 文件类型标签   | File-type badge         | `DocumentRow.vue` `.file-type`                         |
| documents.row-tags               | 行内标签       | Row tags                | `DocumentRow.vue` `.row-tags`                          |
| documents.status-pill            | 健康状态标签   | Health pill             | `DocumentRow.vue` `.status-pill`                       |
| documents.modification-pill      | 源文件状态标签 | Modification pill       | `DocumentRow.vue` `.mod-pill`                          |
| documents.sort-indicator         | 排序指示       | Sort indicator          | `DocumentTable.vue` `.sort-indicator`                  |
| documents.col-resizer            | 列宽调整手柄   | Column resize handle    | `DocumentTable.vue` `.col-resizer`                     |
| documents.row-actions            | 行内操作区     | Row actions             | `DocumentRow.vue` `.row-actions`                       |
| documents.row.open-button        | 打开           | Open                    | `DocumentRow.vue` / `actions.open`                     |
| documents.row.preview-button     | 预览           | Preview                 | `DocumentRow.vue` / `actions.preview`                  |
| documents.row.commit-button      | 提交修改       | Commit modified         | `DocumentRow.vue` / `actions.normalCommit`             |
| documents.row.export-button      | 导出           | Export                  | `DocumentRow.vue` / `actions.export`                   |

## 文档详情与版本历史

| 术语 ID                  | 中文名称           | 英文名称                  | 绑定（组件 / i18n key）                                                                 |
| ------------------------ | ------------------ | ------------------------- | --------------------------------------------------------------------------------------- |
| detail.panel             | 文档详情面板       | Document details panel    | `DocumentsView.vue` `.detail-panel` / `details.label`                                   |
| detail.header            | 详情标题区         | Details header            | `DocumentsView.vue` `.panel-header`                                                     |
| detail.checkout-button   | 切换版本           | Checkout                  | `DocumentsView.vue` / `actions.checkout`                                                |
| detail.pin-button        | 固定详情面板       | Pin panel                 | `DocumentsView.vue` / `details.pinPanel`, `details.unpinPanel`                          |
| detail.version-history   | 版本历史           | Version history           | `VersionHistoryPanel.vue` / `details.versionHistory`                                    |
| detail.version-count     | 总版本数           | Total versions            | `VersionHistoryPanel.vue` / `details.totalVersions`                                     |
| detail.view-mode         | 版本历史视图切换   | Version-history view mode | `VersionHistoryPanel.vue` `.segmented-control` / `details.listView`, `details.treeView` |
| detail.version-row       | 版本行             | Version row               | `VersionHistoryPanel.vue` `.version-row`                                                |
| detail.version-status    | 版本状态标签       | Version status            | `VersionHistoryPanel.vue` `.version-status`                                             |
| detail.based-on          | 基于版本           | Based on                  | `VersionHistoryPanel.vue` / `details.basedOnVersion`                                    |
| detail.version-graph     | 版本树图           | Version graph             | `VersionGraph.vue`                                                                      |
| detail.graph-toolbar     | 版本树工具栏       | Graph toolbar             | `VersionHistoryPanel.vue` `.graph-toolbar`                                              |
| detail.graph-reset       | 重置视图           | Reset view                | `VersionHistoryPanel.vue` / `actions.resetView`                                         |
| detail.graph-maximize    | 最大化             | Maximize                  | `VersionHistoryPanel.vue` / `actions.maximize`                                          |
| detail.graph-minimize    | 最小化             | Minimize                  | `GraphMaximized.vue` / `actions.minimize`                                               |
| detail.note              | 版本备注           | Version note              | `VersionDetailSection.vue` / `details.note`                                             |
| detail.note-edit         | 备注编辑按钮       | Note edit button          | `VersionDetailSection.vue` `.note-edit-hint` / `details.noteEditHint`                   |
| detail.tags              | 标签区             | Tags                      | `DocumentMetaSection.vue` / `tags.title`                                                |
| detail.tag-chip          | 标签条             | Tag chip                  | `DocumentMetaSection.vue` `.tag-chip`                                                   |
| detail.tag-add           | 添加标签           | Add tag                   | `DocumentMetaSection.vue` `.tag-add-btn` / `tags.addPlaceholder`                        |
| detail.tag-input         | 标签输入框         | Tag input                 | `DocumentMetaSection.vue` `.tag-input`                                                  |
| detail.tag-remove        | 移除标签           | Remove tag                | `DocumentMetaSection.vue` `.tag-remove`                                                 |
| detail.maximized-overlay | 版本树最大化覆盖层 | Maximized graph overlay   | `GraphMaximized.vue`                                                                    |
| detail.maximized-stage   | 版本树舞台         | Graph stage               | `GraphMaximized.vue` `.graph-stage`                                                     |
| detail.maximized-context | 最大化详情侧栏     | Maximized context panel   | `GraphMaximized.vue` `.graph-context`                                                   |

## 右键菜单

| 术语 ID                      | 中文名称       | 英文名称                    | 绑定（组件 / i18n key）                   |
| ---------------------------- | -------------- | --------------------------- | ----------------------------------------- |
| menu.doc                     | 文档右键菜单   | Document context menu       | `DocRowContextMenu.vue`                   |
| menu.doc.preview             | 预览           | Preview                     | `source.preview`                          |
| menu.doc.open                | 打开           | Open                        | `source.open`                             |
| menu.doc.export              | 导出           | Export                      | `actions.export`                          |
| menu.doc.commit              | 提交修改       | Commit modified             | `source.commitModified`                   |
| menu.doc.replace-commit      | 替换提交       | Replace commit              | `source.replaceCommit`                    |
| menu.doc.rename              | 重命名         | Rename                      | `source.rename`                           |
| menu.doc.remove-from-project | 移出项目       | Remove from project         | `source.removeFromProject`                |
| menu.doc.delete              | 删除           | Delete                      | `source.delete`                           |
| menu.doc.refresh             | 刷新           | Refresh                     | `actions.refresh`                         |
| menu.doc.properties          | 属性           | Properties                  | `source.properties`                       |
| menu.version                 | 版本右键菜单   | Version context menu        | `VersionContextMenu.vue`                  |
| menu.version.preview         | 预览版本       | Preview version             | `versionMenu.preview`                     |
| menu.version.export          | 导出版本       | Export version              | `versionMenu.export`                      |
| menu.version.compare-latest  | 与最新版本对比 | Compare with latest version | `versionMenu.compareLatest`               |
| menu.version.checkout        | 切换版本       | Switch version              | `versionMenu.checkout`                    |
| menu.version.delete          | 删除版本       | Delete version              | `versionMenu.delete`                      |
| menu.version.refresh         | 刷新           | Refresh                     | `actions.refresh`                         |
| menu.preview                 | 预览右键菜单   | Preview context menu        | `DocumentPreview.vue`                     |
| menu.preview.reload          | 重新加载       | Reload                      | `preview.reload`                          |
| menu.app                     | 全局右键菜单   | App context menu            | `AppContextMenu.vue`                      |
| menu.app.refresh             | 刷新           | Refresh                     | `actions.refresh`                         |
| menu.app.inspect             | 检查元素       | Inspect                     | `contextMenu.inspect`（仅开发者模式可见） |

## 对话框

模态对话框共用 `BaseModal.vue`，结构术语如下：

| 术语 ID         | 中文名称         | 英文名称            | 绑定（组件 / i18n key）          |
| --------------- | ---------------- | ------------------- | -------------------------------- |
| dialog.overlay  | 对话框遮罩       | Modal overlay       | `BaseModal.vue` `.modal-overlay` |
| dialog.panel    | 对话框面板       | Modal panel         | `BaseModal.vue` `.modal-panel`   |
| dialog.title    | 对话框标题       | Dialog title        | `BaseModal.vue` `.modal-heading` |
| dialog.subtitle | 对话框副标题     | Dialog subtitle     | `BaseModal.vue` `.modal-heading` |
| dialog.close    | 对话框关闭按钮   | Dialog close button | `BaseModal.vue` / `dialog.close` |
| dialog.footer   | 对话框底部操作区 | Dialog footer       | `BaseModal.vue` `.modal-footer`  |

各对话框：

| 术语 ID                                | 中文名称           | 英文名称                   | 绑定（组件 / i18n key）                                                           |
| -------------------------------------- | ------------------ | -------------------------- | --------------------------------------------------------------------------------- |
| dialog.add-document                    | 添加文档对话框     | Add-document dialog        | `AddDocumentDialog.vue` / `addDocument.title`                                     |
| dialog.add-document.project-select     | 导入目录           | Import target              | `addDocument.projectLabel`                                                        |
| dialog.add-document.file-field         | 文件               | File                       | `addDocument.fileLabel`                                                           |
| dialog.add-document.browse             | 浏览               | Browse                     | `addDocument.browse`                                                              |
| dialog.add-document.name-field         | 文档名称           | Document name              | `addDocument.nameLabel`                                                           |
| dialog.add-document.author-field       | 作者               | Author                     | `addDocument.authorLabel`                                                         |
| dialog.add-document.import-card        | 导入卡片           | Import card                | `AddDocumentDialog.vue` `.import-card`                                            |
| dialog.add-document.remove-file        | 移除               | Remove                     | `addDocument.removeFile`                                                          |
| dialog.add-document.bulk-block         | 大批量导入说明     | Bulk import block          | `AddDocumentDialog.vue` `.bulk-block` / `addDocument.bulkHint`                    |
| dialog.add-document.bulk-title         | 批量导入标题       | Bulk-import title          | `addDocument.bulkTitle`                                                           |
| dialog.add-document.progress           | 导入进度           | Import progress            | `AddDocumentDialog.vue` / `addDocument.progress`                                  |
| dialog.add-document.drop-hint          | 窗口拖放导入提示   | Window-drop import hint    | `addDocument.dropHint`                                                            |
| dialog.add-document.submit             | 添加               | Add                        | `addDocument.submit`                                                              |
| dialog.add-document.import-all         | 全部导入           | Import all                 | `addDocument.importAll`                                                           |
| dialog.commit-modified                 | 提交修改对话框     | Commit-modified dialog     | `CommitModifiedDialog.vue` / `commitModified.title`                               |
| dialog.commit-modified.doc             | 文档               | Document                   | `commitModified.docLabel`                                                         |
| dialog.commit-modified.note            | 备注               | Note                       | `commitModified.noteLabel`                                                        |
| dialog.commit-modified.submit          | 提交               | Commit                     | `commitModified.submit`                                                           |
| dialog.rename                          | 重命名对话框       | Rename dialog              | `RenameDialog.vue` / `renameDialog.title`                                         |
| dialog.rename.name                     | 新名称             | New name                   | `renameDialog.nameLabel`                                                          |
| dialog.rename.submit                   | 重命名             | Rename                     | `renameDialog.submit`                                                             |
| dialog.note-edit                       | 编辑版本备注对话框 | Edit-version-note dialog   | `NoteEditDialog.vue` / `noteEditDialog.title`                                     |
| dialog.note-edit.note                  | 备注               | Note                       | `noteEditDialog.noteLabel`                                                        |
| dialog.note-edit.submit                | 保存               | Save                       | `noteEditDialog.submit`                                                           |
| dialog.new-document                    | 新建文档对话框     | New-document dialog        | `NewDocumentDialog.vue` / `newDocument.title`                                     |
| dialog.new-document.format             | 格式               | Format                     | `newDocument.formatLabel`                                                         |
| dialog.new-document.aspect-ratio       | 幻灯片比例         | Slide ratio                | `newDocument.aspectRatioLabel`                                                    |
| dialog.new-document.name               | 文档名称           | Document name              | `newDocument.nameLabel`                                                           |
| dialog.new-document.submit             | 创建               | Create                     | `newDocument.submit`                                                              |
| dialog.document-status                 | 文档属性对话框     | Document-properties dialog | `DocumentStatusDialog.vue` / `source.properties`                                  |
| dialog.document-status.modification    | 修改状态           | Modification status        | `source.status`                                                                   |
| dialog.document-status.source-path     | 源路径             | Source path                | `source.path`                                                                     |
| dialog.document-status.document-id     | 文档 ID            | Document ID                | `details.documentId`                                                              |
| dialog.document-status.backend         | 备份后端           | Backup backend             | `details.backend`                                                                 |
| dialog.document-status.project-select  | 所属项目           | Project membership         | `projects.title`                                                                  |
| dialog.compare                         | 对比选择对话框     | Compare-selection dialog   | `DocumentCompareDialog.vue` / `compare.title`                                     |
| dialog.compare.old-side                | 旧文档侧           | Old side                   | `compare.oldDoc`                                                                  |
| dialog.compare.new-side                | 新文档侧           | New side                   | `compare.newDoc`                                                                  |
| dialog.compare.doc-select              | 文档选择           | Document select            | `compare.docLabel`                                                                |
| dialog.compare.version-select          | 版本选择           | Version select             | `compare.versionLabel`                                                            |
| dialog.compare.run                     | 开始对比           | Run comparison             | `compare.run`                                                                     |
| dialog.compare.hint                    | 对比限制提示       | Compare limitation hint    | `compare.docxOnlyHint`, `compare.identicalSelection`                              |
| dialog.switch-backend                  | 仓库连接对话框     | Vault-connect dialog       | `SwitchBackendDialog.vue` / `connect.title`                                       |
| dialog.switch-backend.dir              | 仓库目录           | Vault directory            | `connect.dirLabel`                                                                |
| dialog.switch-backend.browse           | 浏览               | Browse                     | `connect.browse`                                                                  |
| dialog.switch-backend.backend          | 后端类型           | Backend                    | `connect.backend`                                                                 |
| dialog.switch-backend.password         | 仓库密码           | Repository password        | `connect.password`                                                                |
| dialog.switch-backend.password-confirm | 确认仓库密码       | Confirm password           | `connect.passwordConfirm`                                                         |
| dialog.switch-backend.submit           | 连接 / 初始化      | Connect / Initialize       | `connect.submit`                                                                  |
| dialog.quick-link                      | 常用链接对话框     | Quick-link dialog          | `QuickLinkDialog.vue` / `quickLinks.dialogAddTitle`, `quickLinks.dialogEditTitle` |
| dialog.quick-link.url                  | 网址               | URL                        | `quickLinks.dialogUrlLabel`                                                       |
| dialog.quick-link.name                 | 名称               | Name                       | `quickLinks.dialogTitleLabel`                                                     |
| dialog.quick-link.fetch                | 获取标题与图标     | Fetch title & icon         | `quickLinks.fetch`                                                                |
| dialog.quick-link.submit               | 确定               | Save                       | `quickLinks.dialogSave`                                                           |
| dialog.native-confirm                  | 原生确认对话框     | Native confirm dialog      | `useVault.ts` `confirmDialog` / `confirm.*`                                       |

## 预览与对比

| 术语 ID             | 中文名称         | 英文名称                      | 绑定（组件 / i18n key）                                            |
| ------------------- | ---------------- | ----------------------------- | ------------------------------------------------------------------ |
| preview.overlay     | 预览覆盖层       | Preview overlay               | `DocumentPreview.vue` `.preview-overlay`                           |
| preview.modal       | 预览窗口         | Preview window                | `DocumentPreview.vue` `.preview-modal` / `preview.title`           |
| preview.subtitle    | 预览副标题       | Preview subtitle              | `preview.subtitle`                                                 |
| preview.close       | 关闭预览         | Close preview                 | `preview.close`                                                    |
| preview.body        | 预览内容区       | Preview body                  | `DocumentPreview.vue` `.preview-body`                              |
| preview.content     | 预览渲染内容     | Preview content               | `DocumentPreview.vue` `.preview-content`                           |
| preview.loading     | 预览加载中       | Preview loading               | `preview.loading`                                                  |
| preview.refreshing  | 最新预览刷新提示 | Latest-preview refresh status | `DocumentPreview.vue` `.preview-refreshing` / `preview.refreshing` |
| preview.error       | 预览错误         | Preview error                 | `preview.error`                                                    |
| preview.unsupported | 暂不支持预览     | Preview not supported         | `preview.unsupportedTitle`, `preview.notSupported`                 |
| compare.overlay     | 对比覆盖层       | Comparison overlay            | `DocumentCompare.vue` `.preview-overlay`                           |
| compare.modal       | 对比窗口         | Comparison window             | `DocumentCompare.vue` `.preview-modal` / `compare.title`           |
| compare.subtitle    | 对比副标题       | Comparison subtitle           | `compare.resultSubtitle`                                           |
| compare.close       | 关闭对比         | Close comparison              | `compare.close`                                                    |
| compare.content     | 对比内容区       | Comparison content            | `DocumentCompare.vue` `.preview-content`                           |
| compare.loading     | 对比生成中       | Comparison loading            | `compare.loading`                                                  |
| compare.error       | 对比失败         | Comparison failure            | `compare.error`                                                    |

## 回收站视图

| 术语 ID              | 中文名称     | 英文名称                | 绑定（组件 / i18n key）                                     |
| -------------------- | ------------ | ----------------------- | ----------------------------------------------------------- |
| trash.panel          | 回收站面板   | Recycle-bin panel       | `TrashView.vue` `.trash-panel` / `trash.title`              |
| trash.header         | 回收站标题区 | Trash header            | `TrashView.vue` `.panel-header`                             |
| trash.doc-count      | 回收站文档数 | Trash document count    | `trash.count`                                               |
| trash.version-count  | 回收站版本数 | Trash version count     | `trash.versionsCount`                                       |
| trash.empty-trash    | 清空回收站   | Empty recycle bin       | `trash.emptyTrash`                                          |
| trash.doc-table      | 已删除文档表 | Deleted-documents table | `TrashView.vue`                                             |
| trash.version-table  | 已删除版本表 | Deleted-versions table  | `TrashView.vue` `.versions-section` / `trash.versionsTitle` |
| trash.restore-button | 恢复         | Restore                 | `trash.restore`                                             |
| trash.delete-button  | 永久删除     | Permanently delete      | `trash.permanentDelete`                                     |
| trash.empty-state    | 空回收站状态 | Empty-bin state         | `trash.empty`                                               |

## 设置与状态

设置视图的页签与卡片：

| 术语 ID                 | 中文名称   | 英文名称         | 绑定（组件 / i18n key）                                 |
| ----------------------- | ---------- | ---------------- | ------------------------------------------------------- |
| settings.intro          | 设置标题区 | Settings intro   | `SettingsView.vue` `.settings-intro` / `settings.title` |
| settings.tabs           | 设置页签栏 | Settings tab bar | `SettingsView.vue` `.tab-bar`                           |
| settings.tab.status     | 状态页签   | Status tab       | `settings.tabs.status`                                  |
| settings.tab.appearance | 外观页签   | Appearance tab   | `settings.tabs.appearance`                              |

状态页签：

| 术语 ID                       | 中文名称         | 英文名称                 | 绑定（组件 / i18n key）                           |
| ----------------------------- | ---------------- | ------------------------ | ------------------------------------------------- |
| status.vault-card             | 仓库状态卡       | Vault-status card        | `StatusVaultCard.vue` / `status.vaultTitle`       |
| status.switch-button          | 切换…            | Switch…                  | `StatusVaultCard.vue` / `connect.switchAction`    |
| status.metrics                | 仓库指标         | Vault metrics            | `StatusVaultCard.vue` `.stat-grid` / `metrics.*`  |
| status.tasks-panel            | 任务面板         | Tasks panel              | `StatusTasksPanel.vue` / `jobs.title`             |
| status.job-row                | 任务行           | Job row                  | `StatusTasksPanel.vue` `.job-row`                 |
| status.job-kind               | 任务类型         | Job kind                 | `StatusTasksPanel.vue` `.job-kind`                |
| status.job-target             | 任务目标         | Job target               | `StatusTasksPanel.vue` `.job-target`              |
| status.job-progress           | 任务进度条       | Job progress track       | `StatusTasksPanel.vue` `.progress-track`          |
| status.job-status             | 任务状态         | Job status               | `StatusTasksPanel.vue` `.job-status`              |
| status.job-cancel             | 取消任务         | Cancel job               | `StatusTasksPanel.vue` `.job-cancel`              |
| status.activity-log           | 活动日志         | Activity log             | `StatusTasksPanel.vue` `.log-panel` / `log.title` |
| status.log-entry              | 活动日志条目     | Log entry                | `StatusTasksPanel.vue` `ol > li`                  |
| status.log-clear              | 清空日志         | Clear log                | `StatusTasksPanel.vue` / `actions.clear`          |
| status.archive-panel          | 归档仓库面板     | Archive-repository panel | `StatusArchivePanel.vue` / `archive.title`        |
| status.archive.backend        | 当前后端         | Active backend           | `archive.currentBackend`                          |
| status.archive.repository-dir | 仓库目录         | Repository directory     | `archive.repositoryDir`                           |
| status.archive.data-dir       | 暂存目录         | Staging directory        | `archive.dataDir`                                 |
| status.archive.restic-version | 本地版本         | Local version            | `archive.resticVersion`                           |
| status.archive.bundled-binary | 内置二进制       | Bundled binary           | `archive.bundledBinary`                           |
| status.archive.password       | 仓库密码         | Repository password      | `archive.resticPassword`                          |
| status.archive.stats          | 仓库统计         | Repository stats         | `archive.snapshotStats`                           |
| status.archive.snapshots      | 快照数           | Snapshots                | `archive.snapshots`                               |
| status.archive.repo-size      | 仓库大小         | Repository size          | `archive.repoSize`                                |
| status.archive.health-check   | 仓库校验         | Repository check         | `archive.healthCheck`                             |
| status.database-card          | 数据库卡片       | Database card            | `SettingsView.vue` / `settings.databaseSection`   |
| status.database-path          | 数据库路径       | Database path            | `settings.dbPath`                                 |
| status.logging-card           | 日志卡片         | Logging card             | `SettingsView.vue` / `settings.loggingSection`    |
| status.log-level              | 日志级别         | Log level                | `settings.logLevel`                               |
| status.log-file               | 日志文件         | Log file                 | `settings.logFile`                                |
| status.reload-app-card        | 重新加载应用卡片 | Reload-app card          | `settings.reloadApp`                              |

外观页签：

| 术语 ID                         | 中文名称         | 英文名称                  | 绑定（组件 / i18n key）                                                         |
| ------------------------------- | ---------------- | ------------------------- | ------------------------------------------------------------------------------- |
| appearance.theme                | 主题             | Theme                     | `settings.theme`                                                                |
| appearance.theme-control        | 主题分段控件     | Theme segmented control   | `SettingsView.vue` `.theme-control`                                             |
| appearance.language             | 语言             | Language                  | `settings.language`                                                             |
| appearance.language-restart     | 语言重启提示     | Language restart prompt   | `settings.languageRestartTitle`, `settings.restartApp`, `settings.restartLater` |
| appearance.dev-mode             | 开发者模式       | Developer mode            | `settings.devMode`                                                              |
| appearance.double-click         | 双击文档         | Double-click document     | `settings.doubleClick`                                                          |
| appearance.double-click.preview | 双击后预览       | Double-click preview      | `settings.doubleClickPreview`                                                   |
| appearance.double-click.open    | 双击后打开       | Double-click open         | `settings.doubleClickOpen`                                                      |
| appearance.columns              | 表格列设置       | Table columns             | `settings.columnsSection`                                                       |
| appearance.columns-always-on    | 始终显示         | Always shown              | `settings.columnsAlwaysOn`                                                      |
| appearance.columns-reset        | 重置列宽与可见性 | Reset widths & visibility | `settings.columnsReset`                                                         |
| appearance.reset-defaults       | 恢复到默认设置   | Restore default settings  | `settings.resetDefaults`                                                        |

开发者专用（仅开发构建可见，不进入产品规格的生产路径）：

| 术语 ID                  | 中文名称       | 英文名称                  | 绑定（组件 / i18n key）                   |
| ------------------------ | -------------- | ------------------------- | ----------------------------------------- |
| dev.reset-card           | 测试与重置卡片 | Test & reset card         | `StageResetSlider.vue` / `dev.title`      |
| dev.stage-slider         | 阶段滑块       | Stage slider              | `StageResetSlider.vue` / `dev.stageLabel` |
| dev.stage-confirm        | 重置到阶段     | Reset to stage            | `dev.confirmStage`                        |
| dev.qinbixin-environment | 亲笔信站点     | Qinbixin site             | `dev.qinbixin.environment`                |
| dev.qinbixin-accounts    | 测试账号切换   | Quick test-account switch | `dev.qinbixin.quickUsers`                 |

## 任务通知与命令面板

| 术语 ID                  | 中文名称       | 英文名称                | 绑定（组件 / i18n key）                                               |
| ------------------------ | -------------- | ----------------------- | --------------------------------------------------------------------- |
| toast.host               | 任务通知区     | Toast host              | `ToastHost.vue` `.toast-host`                                         |
| toast.item               | 任务通知       | Toast                   | `ToastHost.vue` `.toast`                                              |
| toast.status             | 通知状态       | Toast status            | `toast.running`, `toast.succeeded`, `toast.failed`, `toast.cancelled` |
| toast.dismiss            | 关闭通知       | Dismiss toast           | `toast.dismiss`                                                       |
| palette.overlay          | 命令面板覆盖层 | Command-palette overlay | `CommandPalette.vue`                                                  |
| palette.input            | 命令搜索框     | Command input           | `commandPalette.placeholder`                                          |
| palette.navigation-group | 导航命令组     | Navigation group        | `commandPalette.groupNavigation`                                      |
| palette.action-group     | 操作命令组     | Actions group           | `commandPalette.groupActions`                                         |
| palette.empty            | 无匹配命令     | No matching commands    | `commandPalette.empty`                                                |
| palette.hint             | 快捷键提示     | Shortcut hint           | `commandPalette.hint`                                                 |

## 亲笔信

| 术语 ID                    | 中文名称     | 英文名称          | 绑定（组件 / i18n key）                                            |
| -------------------------- | ------------ | ----------------- | ------------------------------------------------------------------ |
| qinbixin.dialog            | 亲笔信对话框 | Qinbixin dialog   | `QinbixinDialog.vue` / `qinbixin.title`                            |
| qinbixin.inbox             | 收信箱       | Inbox             | `qinbixin.inboxTab`, `qinbixin.inboxTitle`                         |
| qinbixin.outbox            | 发信箱       | Outbox            | `qinbixin.outboxTab`, `qinbixin.outboxTitle`                       |
| qinbixin.compose           | 发信         | Compose           | `qinbixin.composeTab`, `qinbixin.composeTitle`                     |
| qinbixin.conversation-list | 会话列表     | Conversation list | `qinbixin.selectConversation`                                      |
| qinbixin.letter-list       | 信件列表     | Letter list       | `qinbixin.noMessages`                                              |
| qinbixin.login-panel       | 登录面板     | Login panel       | `QinbixinLoginPanel.vue` / `qinbixin.loginTitle`                   |
| qinbixin.account-field     | 账号         | Account           | `qinbixin.userName`                                                |
| qinbixin.password-field    | 密码         | Password          | `qinbixin.password`                                                |
| qinbixin.recipient-select  | 收件人       | Recipient         | `qinbixin.recipient`                                               |
| qinbixin.title-field       | 信件标题     | Letter title      | `qinbixin.titlePlaceholder`                                        |
| qinbixin.content-field     | 写信内容     | Letter content    | `qinbixin.contentPlaceholder`                                      |
| qinbixin.song-field        | 曲名         | Song title        | `qinbixin.songTitle`                                               |
| qinbixin.attachments       | 附件区       | Attachments       | `qinbixin.addImage`, `qinbixin.addVideo`, `qinbixin.addAttachment` |
| qinbixin.send-button       | 发送         | Send              | `qinbixin.send`                                                    |
| qinbixin.reply             | 回复         | Reply             | `qinbixin.reply`                                                   |
| qinbixin.mark-all-read     | 全部已读     | Mark all read     | `qinbixin.markAllRead`                                             |
| qinbixin.logout            | 退出登录     | Log out           | `qinbixin.logout`                                                  |

## 引导与仓库连接

| 术语 ID             | 中文名称       | 英文名称               | 绑定（组件 / i18n key）                  |
| ------------------- | -------------- | ---------------------- | ---------------------------------------- |
| boot.loading        | 连接中提示     | Connecting state       | `App.vue` `.boot-state` / `boot.loading` |
| boot.onboarding     | 引导面板       | Onboarding panel       | `App.vue` `.onboarding` / `boot.welcome` |
| boot.open-error     | 打开仓库错误   | Open-vault error       | `boot.openFailed`                        |
| boot.connect-button | 创建或选择仓库 | Create or select vault | `boot.connect`                           |
| connect.flow        | 仓库连接流程   | Vault-connect flow     | `SwitchBackendDialog.vue`                |
| connect.new-vault   | 新建仓库       | Create vault           | `connect.initialized`                    |
| connect.open-vault  | 打开仓库       | Open vault             | `connect.opened`                         |

## 共享领域词汇

这些不是控件，但讨论和规格中会反复引用，因此一并固定：

| 术语 ID                 | 中文名称   | 英文名称            | 说明                                 |
| ----------------------- | ---------- | ------------------- | ------------------------------------ |
| term.vault              | 仓库       | Vault               | 本地文档库整体                       |
| term.document           | 文档       | Document            | 文档库中的一个业务对象               |
| term.version            | 版本       | Version             | 文档的一个历史版本                   |
| term.current-version    | 当前版本   | Current version     | `status.current`                     |
| term.archived-version   | 已归档版本 | Archived version    | `status.archived`                    |
| term.descendant-version | 衍生版本   | Descendant versions | 版本树中从某版本派生的版本           |
| term.ancestor-version   | 祖先版本   | Ancestor versions   | 恢复被删版本时可能需要同时恢复的版本 |
| term.project            | 项目       | Project             | 侧边栏中的组织单元                   |
| term.source-file        | 源文件     | Source file         | 本机被追踪的工作文件                 |
| term.source-tracking    | 源文件追踪 | Source tracking     | 追踪源文件并检测修改的能力           |
| term.selected-document  | 选中文档   | Selected document   | 当前被文档行选中的对象               |
| term.selected-version   | 选中版本   | Selected version    | 当前被版本历史选中的对象             |
| term.active-project     | 当前项目   | Active project      | 左侧项目树当前作用域                 |
| term.active-section     | 当前视图   | Active section      | 文档 / 回收站 / 设置                 |
| term.job                | 任务       | Job                 | 后台异步工作                         |
| term.quick-link         | 常用链接   | Quick link          | 侧边栏中的外部链接                   |
| term.trash-item         | 回收站条目 | Trash item          | 被移入回收站的文档或版本             |

共享状态词汇与 i18n 对应如下：

| 术语 ID                      | 中文名称   | 英文名称       | i18n key                 |
| ---------------------------- | ---------- | -------------- | ------------------------ |
| state.modification.untracked | 未追踪     | Not tracked    | `modification.none`      |
| state.modification.unchanged | 未修改     | Unchanged      | `modification.unchanged` |
| state.modification.modified  | 已修改     | Modified       | `modification.modified`  |
| state.modification.missing   | 源文件缺失 | Source missing | `modification.missing`   |
| state.health.synced          | 已同步     | Synced         | `status.synced`          |
| state.health.needs-review    | 需检查     | Needs review   | `status.needsReview`     |
| state.version.current        | 当前       | Current        | `status.current`         |
| state.version.archived       | 已归档     | Archived       | `status.archived`        |
| state.job.running            | 运行中     | Running        | `status.running`         |
| state.job.succeeded          | 已完成     | Succeeded      | `status.succeeded`       |
| state.job.failed             | 已失败     | Failed         | `status.failed`          |
| state.job.cancelled          | 已取消     | Cancelled      | `status.cancelled`       |
| state.backend.local          | 本地       | Local          | `backend.restic`         |
| state.backend.local-copy     | 本地复制   | Local copy     | `backend.local-copy`     |

任务类型：

| 术语 ID               | 中文名称 | 英文名称 | i18n key            |
| --------------------- | -------- | -------- | ------------------- |
| job.kind.commit       | 提交     | Commit   | `jobs.commit`       |
| job.kind.export       | 导出     | Export   | `jobs.export`       |
| job.kind.checkout     | 切换版本 | Checkout | `jobs.checkout`     |
| job.kind.delete       | 删除     | Delete   | `jobs.delete`       |
| job.kind.archive      | 压缩归档 | Archive  | `jobs.archive`      |
| job.kind.create-blank | 正在创建 | Creating | `jobs.create_blank` |

## 不推荐用语

| 不推荐                      | 规范说法                | 原因                                          |
| --------------------------- | ----------------------- | --------------------------------------------- |
| 红线                        | 对比 / 对比结果         | Word 修订痕迹只是实现形式；用户目标是生成对比 |
| 存储页签                    | 状态页签                | 旧存储页签已并入“状态”                        |
| 详情栏 / 版本面板           | 文档详情面板 / 版本历史 | 避免把面板整体与其子区域混用                  |
| 右键弹出框                  | 右键菜单                | 与模态对话框区分                              |
| 上面那个按钮 / 右侧那个面板 | 对应术语 ID             | 位置描述不稳定，无法跨讨论复用                |
