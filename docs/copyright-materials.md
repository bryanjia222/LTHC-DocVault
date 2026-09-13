# 软著程序材料导出

在仓库根目录运行：

```powershell
uv run scripts/export_copyright.py
```

需要 Git 和 uv。脚本声明了 Python 3.11 及以上版本和固定的
`python-docx==1.2.0` 依赖；uv 首次运行时自动准备环境。脚本可以从其他目录
通过绝对路径启动，默认仍读取脚本所在仓库。

每次生成到 `dist/copyright-materials/export-日期-时间/`，包含：

- `DocVault-源程序材料.docx`：源码正文、软件名称、版本及页码。
- `source-statistics.txt`：可复制到登记资料的行数统计。
- `source-statistics.json`：逐文件统计、文件顺序、SHA-256、Git HEAD 和工作区状态。
- `source-line-index.csv`：材料页码、页内行号与原文件行号的对应关系。
- `generation.log`：本次运行日志，失败时包含错误上下文。

软件版本自动读取 `apps/desktop/package.json`。脚本不会修改登记资料文档，
后续需要把新统计的源程序量填入登记资料；其他登记字段可继续沿用。

## 常用参数

```powershell
# 只统计行数，不生成 Word 和行号索引
uv run scripts/export_copyright.py --stats-only

# 同时包含新增但尚未 git add、且未被 Git 忽略的源码
uv run scripts/export_copyright.py --include-untracked

# 使用指定的软件名称、申报版本和输出目录（目录必须为空）
uv run scripts/export_copyright.py --name "兰天嗨彩办公文档管理" --version "1.0.0" --output-dir "dist/copyright-v1"

# 长行在纵向页面中容纳不下时，使用横向 A4
uv run scripts/export_copyright.py --landscape
```

`--root` 可指定其他 DocVault 工作副本；默认统计当前工作区的文件内容，
包括已修改但未提交的内容，并非直接从 Git 提交对象读取。已在工作区删除的
跟踪文件会跳过并记入日志。源码必须是仓库内的 UTF-8 普通文件。

## 统计与排版口径

沿用最初材料的范围：`apps`、`crates`、`shared` 下 `src` 目录内的
`.rs`、`.ts`、`.vue`、`.css`、`.js`、`.html` 文件。默认仅包含 Git 跟踪文件。
独立测试目录、`tests.rs`、`.test.*`、`.spec.*`、依赖和构建目录不计入。
源码里的注释、开发代码和内嵌测试保留。这个统计是**非空行数**，不是去掉
注释、开发代码后的有效代码行数；JSON 也提供包含空行的物理行数。

文件按共享类型、核心、存储、OOXML、任务、CLI、桌面后端、插件、共享插件
前端、桌面前端排序，各组内部按路径排序，新增的其他模块排在末尾。
完整文件清单保存在 JSON 中。提取时仅去掉空行，打印时将制表符按 4 列展开，
其余源码文本保留，材料行号与源码行号通过 CSV 对照。

超过 3,000 个非空行时提取连续前 1,500 行和后 1,500 行；不超过时输出全部，
不会重复输出重叠片段。每页设置 50 个源码行，全文输出的末页可不足 50 行。
没有源码时会明确报错。

程序材料采用 A4 页面，正文使用 1.5 倍行距，显式分页，并按行宽缩小长行字体，最低 5 磅；
若低于该字号仍无法容纳则报错，不截断或静默删行。可使用横向布局，或先在
源码中合理换行后重新导出。字体使用 Consolas 和宋体，跨平台字体替换可能
改变换行；请在 Word、WPS 或 LibreOffice 中检查实际页数、字体及长行后提交。
脚本中的计划页数和结构检查不能替代最终分页预览。

输出目录必须为空，以避免覆盖已打开的文档或之前的材料。失败导出的目录
可能包含统计和日志，但不代表导出完成；查看 `generation.log` 后重新运行，
默认会自动使用新目录。

## 验证脚本

```powershell
uv run --with python-docx==1.2.0 python -m unittest discover -s scripts -p test_export_copyright.py
```

测试覆盖文件筛选、非空行与原始行号、3,000 行提取边界、Word 正文和分页结构、
超长行失败及已有文件保护。
