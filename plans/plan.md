# DocVault 全流程审计修复计划（三批）

依据代码审计的 11 项问题，分三批修复。每批独立可验证（fmt/clippy/test + npm lint/build）并单独提交。全程遵守 AGENTS.md：复用优先、最小改动、单一 restic 发现机制、storage/jobs 保持 Tauri-free、无 speculative 架构、无兼容 shim。

公共验证命令（每批结束都跑）：
- Rust（workspace + 桌面，测试需清环境以避开 `DOCVAULT_*` 覆盖）：
  `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
  `env -u DOCVAULT_ROOT_DIR -u DOCVAULT_DATA_DIR -u DOCVAULT_DB_PATH -u DOCVAULT_BACKUP_BACKEND -u DOCVAULT_RESTIC_PATH -u DOCVAULT_RESTIC_PASSWORD cargo test --workspace`
  `env -u DOCVAULT_ROOT_DIR -u DOCVAULT_DATA_DIR -u DOCVAULT_DB_PATH -u DOCVAULT_BACKUP_BACKEND -u DOCVAULT_RESTIC_PATH -u DOCVAULT_RESTIC_PASSWORD cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
- 前端：`cd apps/desktop && npm run lint && npm run build`

---

## 第一批：真实性 / 状态一致性契约（#2 #3 #4 #5 #6）

目标：让前端始终真实反映 vault 与 job 的真实状态，消除"静默失败"和"卡 Running"。对应 AGENTS §6.2（No silent failures）、§8（Avoid panics in production paths）。

### #2 job runner 捕获 panic（始终发终态事件）
- 文件：`crates/jobs/src/lib.rs`
- `spawn` 的线程体内用 `std::panic::catch_unwind(AssertUnwindSafe(|| work(&progress)))` 包裹 `work(&progress)`。
- `catch_unwind` 返回 `Ok(Ok(()))` -> Succeeded；`Ok(Err(e))` -> Failed(e)；`Err(panic)` -> Failed(panic 消息，`panic.payload()` 取 `&str`/`String`，否则 `"job panicked"`）。
- 无论哪种情况都走到既有的终态写入 + `on_event(terminal)`，保证终态事件必发。
- 测试：`spawn` 一个 `work` 闭包内 `panic!("boom")`，断言 job 到达 `Failed` 且 `error` 含 "boom"，终态计数器 +1（复用现有 `make_counter` 模式）。

### #3 暴露 `open_if_initialized` 的打开失败
- 文件：`apps/desktop/src-tauri/src/state.rs`、`dto.rs`、`commands.rs`；前端 `composables/useVault.ts`、`App.vue`、`i18n/locales/{zh-CN,en-US}.ts`
- `AppState` 增加 `last_open_error: Arc<Mutex<Option<String>>>`。
- `open_if_initialized`：把 `if let Ok(storage) = VaultStorage::open(paths)` 改为匹配 `Err(e)` 时写入 `last_open_error`（清空于成功 init/connect）。
- `VaultStatusDto` 增加 `open_error: Option<String>`；`vault_status` 读取并返回。
- 前端 `VaultStatus` 接口加 `open_error`，`refreshStatus` 存到新的 `openError` ref；onboarding 区块在 `openError` 非空时显示该错误（`boot.openFailed` 文案）。
- 测试（桌面）：构造一个 config 可解析但 `VaultStorage::open` 失败的场景（如 db 路径不可写 / 迁移失败），断言 `open_if_initialized` 后 `last_open_error` 为 `Some`。若难构造，至少加 `vault_status` 透传字段的单元测试。

### #4 读命令对 poisoned mutex 不再 panic
- 文件：`apps/desktop/src-tauri/src/state.rs`、`commands.rs`
- 在 `state.rs` 加 `fn lock_vault(state: &AppState) -> Result<std::sync::MutexGuard<...>, String>`，遇 `PoisonError` 用 `into_inner()` 恢复（读路径 best-effort）并 `tracing::warn!` 记录，不 panic。
- `vault_status`/`list_documents_with_versions`/`get_config` 改用该 helper（替换 `.lock().expect("vault mutex poisoned")`）。
- 写路径（`jobs.rs::execute_*`）已是 `.map_err`，保持不变（poison 时 job 优雅 Failed）。
- 与 #2 协同：#2 阻止 job 线程 panic 毒化 mutex，#4 让读命令即便遇到毒化也不级联崩溃。
- 测试：用 ` PoisonError` 难直接构造；以 #2 的 panic 测试间接覆盖"毒化后读命令不 panic"（在 #2 测试后调用一次 `list_documents_with_versions` 等价路径，断言返回 `Err` 而非 panic）。若不可行则 documented。

### #5 切换 vault 时清空 jobs registry
- 文件：`crates/jobs/src/lib.rs`、`apps/desktop/src-tauri/src/state.rs`
- `JobRegistry` 增加 `pub fn clear(&self)`（清 `records` + `order`）。
- `connect_vault_core` 在成功 init/open 后、返回前调用 `state.jobs.clear()`（此时已确认无 Running job，清空安全）。
- 前端 `connect()` 已在 `Promise.all` 中 `loadJobs()`，会拿到清空后的列表，无需额外改动。
- 测试（桌面）：先 spawn 一个终态 job，再 `connect_vault_core` 成功，断言 `state.jobs.list()` 为空。

### #6 `vault_status` 的 initialized 与 root_dir 一致
- 文件：`apps/desktop/src-tauri/src/state.rs`、`commands.rs`
- `vault_status` 的 `root_dir`：vault 已打开时取 `vault.paths().root_dir`，否则取 `current_root(app)`（意图中的根）。
- `init_vault`（onboarding）改用 `current_root(&app)` 而非 `VaultPaths::default_root()`，尊重上次选择。需把 `app: &AppHandle` 传入 `init_vault`（命令层传入）。
- 这样 onboarding init、connect、open 三条路径的根目录来源统一为 `current_root`。
- 测试：现有 `connect_*` 测试不变；新增 `init_vault_uses_current_root`（设 pref 后 init，断言落在 pref 路径）——若 pref 注入不便，则用 `vault_status` 字段断言。

提交信息：`fix(desktop): truthful vault/job state (panic-safe runner, open errors, poison-safe reads, clear jobs on switch, root consistency)`

---

## 第二批：restic 可用性 + 存储层收尾（#7 #8 #9）

目标：让 restic 后端在正常目录可用；消除 `get_config` 重复 spawn；写操作 label 不再全量拉文档。对应 AGENTS §5.4（restic 查找顺序）、§5.3（不持久化本地路径——restic_path 属 config 非数据库，允许）。

### #7 补全 §5.4 restic 查找顺序（单一机制、可测试）
- 文件：`crates/storage/src/config.rs`、`apps/desktop/src-tauri/tauri.conf.json`
- 现状 `bundled_or_system_restic` 只做第 4 步（`root_dir.parent()/third_party/...`）+ 第 5 步（PATH），缺第 3 步"packaged sidecar"。
- 重构为 `fn bundled_or_system_restic(paths: &VaultPaths, search_roots: &[PathBuf]) -> PathBuf`：按顺序在每个 root 下找 `third_party/restic/0.19.1/<triple>/<bin>`，再回退 PATH。薄封装 `discover_restic(paths)` 用 `std::env::current_exe().parent()` + `paths.root_dir.parent()` 组成 `search_roots` 调用之（storage 保持 Tauri-free，仅用 std）。
- 查找顺序对齐 §5.4：configured(1) -> env(2) -> 既有逻辑内 packaged/exe-adjacent(3) -> third_party asset(4) -> PATH(5)。`read_settings` 调用点改为 `discover_restic(paths)`。
- `tauri.conf.json` `bundle` 增加 `resources`：把 `../../../third_party/restic/**`（或选定 triple）拷入打包产物，使 packaged 构建里 exe 同级可发现 restic。
- dev 模式：exe 在 `target/debug/`，walk-up 找 `third_party` 命中仓库根（dev 仍要求 vault 在仓库内或 restic 在 PATH，沿用既有说明）。
- 测试（storage）：`discover_restic_with_search_roots_finds_bundled`——tempdir 造 `third_party/restic/0.19.1/<triple>/<bin>`，传入该 tempdir 作为 search_root，断言命中；`falls_back_to_path_when_absent`——无文件时返回 bare `restic.exe`。`<triple>` 用 `target_triple()`。
- 文档/memory：更新 restic 发现说明（exe-adjacent + third_party walk-up + PATH）。

### #8 缓存 `restic_version`
- 文件：`crates/storage/src/lib.rs`、`apps/desktop/src-tauri/src/commands.rs`
- `VaultStorage` 增加 `restic_version: OnceCell<String>`，在 `init`/`open` 末尾按 backend 计算（仅 `Restic` 时 spawn 一次 `restic version`；`LocalCopy` 时为空串），暴露 `pub fn restic_version(&self) -> &str`。
- `get_config` 改读 `vault.restic_version()`，删除 `commands.rs::restic_version` 每次调用 spawn 的逻辑（保留为 VaultStorage 内部实现）。
- 效果：每个 vault 会话只 spawn 一次 restic 进程，`get_config` 不再阻塞。
- 测试：现有 restic 测试覆盖 init 路径；新增 `restic_version_cached`（mock restic，init 后多次 `restic_version()` 返回同值且进程只被调一次——用日志计数断言）。

### #9 写操作 label 用定向查询
- 文件：`crates/storage/src/{lib,sqlite}.rs`、`apps/desktop/src-tauri/src/jobs.rs`
- storage 增加 `pub fn document_name(&self, id: &str) -> StorageResult<String>`（按 id 定向 SQL 查询，缺失返回 `DocumentNotFound`）。
- `jobs.rs::lookup_document_name` 改调 `vault.document_name(id)`，替换 `vault.list_documents()` 全量扫描。
- 测试：`document_name_returns_existing` / `document_name_missing`（storage 层）；现有桌面 job 测试不变。

提交信息：`feat(storage): complete restic discovery order, cache restic version, targeted document lookup`

---

## 第三批：jobs 运行时健壮 + 前端收尾（#1 #10 #11）

目标：上云场景下 restic 卡顿可超时/取消，job 不再永久 Running；裁剪 job 历史；活动日志补终态。对应 AGENTS §3.3（cancel/timeout 为显式需求，非 speculative，保持具体实现无插件抽象）。

### #1 restic 超时 + job 取消
- 文件：`crates/jobs/src/lib.rs`、`crates/storage/src/{restic,error}.rs`、`apps/desktop/src-tauri/src/jobs.rs`；前端 `composables/useVault.ts`、`components/views/JobsView.vue`、`i18n/locales`
- **jobs crate**：
  - 增加 `JobStatus::Cancelled`（`#[serde(rename_all="lowercase")]` 已有，加变体）。
  - `spawn` 的 `work` 签名扩展为 `FnOnce(&dyn Fn(Option<f64>), &CancelToken) -> Result<(), String>`，`CancelToken = Arc<AtomicBool>`。
  - registry 内部存 `HashMap<JobId, CancelToken>`；增加 `pub fn cancel(&self, id: &str) -> bool`（置位 token）。cancel 后线程在下一检查点把状态置 `Cancelled` 并发终态事件。
  - 终态匹配：`Ok`->Succeeded、`Err`->Failed、`Cancelled`（在 executor 检查到取消时返回特定 `Err` 或专用路径）——为简单起见，executor 检测取消时返回 `Err("__cancelled__".into())`，runner 识别该 sentinel 转 `Cancelled`；或更干净：work 返回新枚举。**采用 sentinel 字符串过于 hacky**；改为：`CancelToken::check()` 在 restic 层抛 `StorageError::Cancelled`，executor 把 `StorageError::Cancelled` 映射为特殊 `Err`，runner 识别 `ResticError::Cancelled`/`StorageError::Cancelled` 转 `Cancelled` 状态。最终方案：work 仍返回 `Result<(), String>`，runner 无法区分取消与失败——故**采用 `Result<(), JobOutcome>` 其中 `JobOutcome::Cancelled`**，或保留 `Result<(), String>` 并让 runner 在 cancel 标志已置位时优先标 `Cancelled`（无论 work 返回什么）。**选定最简：runner 在终态时若 `cancel_token` 已置位，则状态为 `Cancelled`（error=None），否则按 work 结果 Succeeded/Failed。** 这避免改 work 签名的返回类型，且语义正确（取消优先）。
- **storage restic.rs**：
  - `run_restic_command` 由 `command.output()?` 改为 `command.spawn()` 得 `Child`，循环 `child.try_wait()` + `thread::sleep`，每轮检查 `cancel_token`（新增参数 `cancel: &CancelToken`）与超时；取消则 `child.kill()`+返回 `Err(ResticError::Cancelled)`；超时则 kill+`Err(ResticError::TimedOut)`。
  - `ensure_restic_repo`/`restic_backup`/`restic_restore` 透传 `cancel`。`archive_source`/`export_resolved_version`/`restore_restic_version` 链路透传。
  - 超时常量：`const RESTIC_TIMEOUT: Duration = 10 * 60`（备份/恢复）；`cat config`/`init` 用较短 `60s`。常量集中定义。
  - `ResticError` 增加 `Cancelled`、`TimedOut` 变体。
- **desktop jobs.rs**：executor 闭包接收 `cancel: &CancelToken`，传入 `vault.commit_document(..., cancel)` 等（core/storage 方法签名加 `cancel: &CancelToken`）。新增命令 `#[tauri::command] pub fn cancel_job(state, job_id: String) -> Result<(), String>` 调 `state.jobs.cancel(&job_id)`，注册到 `invoke_handler`。
- **core 透传**：`DocVault::commit_document/export_version/checkout_version` 增加 `cancel: &CancelToken` 参数转发给 storage。检查 core 现有签名，统一加。
- **前端**：`JobsView` 对 `running` job 显示"取消"按钮，调 `invoke("cancel_job", { jobId })`（注意 camelCase 与 `rename_all`——该命令无多词参数，`job_id` 单词不冲突，但为一致加 `rename_all="snake_case"`）。`useVault` 加 `cancelJob`。`Job` 类型 status 加 `"cancelled"`；`mapJob` 处理；i18n 加 `status.cancelled`、`actions.cancel`、`jobs.cancelled` 文案。
- **测试**：复用 `write_mock_restic` 增加 `MockRestic::Hang`（脚本 `pause`/`timeout` 模拟挂起）。spawn commit job -> 立即 `cancel` -> 断言 job 到达 `Cancelled`（非 Failed）、error=None、finished_at 有值。另加超时测试（`Hang` + 极短超时常量——超时常量需可注入测试值，故把超时作为 `CancelToken` 旁的参数或 `ResticRunner::with_timeout` 构造；为可测，`run_restic_command` 接受 `timeout: Duration` 参数）。
- 注意：超时参数化以保可测；生产值用常量默认。

### #10 job 历史裁剪
- 文件：`crates/jobs/src/lib.rs`
- `RegistryInner` 保留 `MAX_RECORDS = 200`。`spawn` 插入后若 `records.len() > MAX_RECORDS`，从 `order` 头部（最老）移除并从 `records` 删除，但**优先保留 `Running`**（running 不裁剪）。`list()` 不变。
- 测试：spawn 201 个终态 job，断言 `list().len() == 200` 且最老被丢。

### #11 活动日志补 job 终态
- 文件：`apps/desktop/src/composables/useVault.ts`、`i18n/locales/{zh-CN,en-US}.ts`
- `subscribeJobs` 在终态（succeeded/failed/cancelled）时调用 `useActivityLog().log(t(...))`：succeeded -> `log.jobSucceeded`（含 target）；failed -> `log.jobFailed`（含 target+error）；cancelled -> `log.jobCancelled`。
- 解决"活动日志永远停在已启动"的小缺口。
- i18n 加对应键。无新测试（前端无测试基建，靠 lint+build）。

提交信息：`feat(jobs): restic timeout + cancellation, prune history, terminal activity log`

---

## 顺序与依赖
- 第一批先行：奠定真实性契约，后续两批在其之上。
- 第二批独立于第一批的 desktop 改动（主要在 storage），可并行思考但按序提交。
- 第三批依赖第一批的 runner 改动（catch_unwind、Cancelled 状态在 `spawn` 终态逻辑上扩展）；#1 的 runner 终态优先级规则与 #2 的 catch_unwind 共存。

## 范围外 / 后续
- restic 进度回流（`percent_done` 流式 -> `progress` 字段）仍为未来项（jobs.rs 已注释），第三批仅做超时/取消，不做进度。
- 取消 UI 仅 JobsView 按钮；不在文档详情页加取消。
- 不引入 job 队列/重试（AGENTS §3.3，非显式需求）。
