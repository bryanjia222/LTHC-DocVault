# DocVault Office 插件(Office.js 任务窗格)

让用户在 **Word / Excel / PowerPoint** 里直接把当前文档保存进 DocVault,不必切出编辑器。

任务窗格 UI 与桥 API 都由 **DocVault 桌面端内嵌的 localhost 桥**(`127.0.0.1:8765`)提供;本目录只含 Office 清单(manifest)与旁加载说明。

## 前置条件

1. **DocVault 桌面端已运行** 且已连接仓库(桥在 `127.0.0.1:8765` 监听;未运行/未开仓库时窗格会提示)。
2. **任务窗格已构建**:在 `shared/addin-web` 执行 `npm install && npm run build`,产物 `dist/` 由桥在 `GET /` 托管并注入会话 token。
3. **Office 桌面版**(Word / Excel / PowerPoint 任一),Windows。

## 旁加载

### 方式一:上传我的加载项(最简单)

1. 启动 DocVault 桌面端(桥已监听 `8765`)。
2. Word/Excel/PPT →「插入」→「加载项」→「我的加载项」→「**上传我的加载项**」→ 选择本目录的 `manifest.xml`。
3. 加载后,在「我的加载项」里点击「保存到 DocVault」打开任务窗格。

### 方式二:共享目录目录清单(局域网多机)

把本目录放到一个共享/UNC 路径,在 Office「信任中心 → 受信任的加载项目录」中添加该共享,重启 Office 后在「我的加载项」中启用。

## 使用

1. 打开一个文档,打开任务窗格。
2. 选择「**新增文档**」(默认)或「**提交新版本**」(从下拉选库内已有文档,可填备注)。
3. 点「保存到 DocVault」→ 文档经桥入库,DocVault 列表自动出现(两阶段提交:先同步、后后台压缩)。

**>20MB 的文件**:Office.js `getFileAsync` 硬上限约 20MB,插件会提示改用 DocVault 的「添加文档」手动导入。

## ⚠️ Spike:localhost SourceLocation 验证(必做)

本 manifest 的 `SourceLocation` 用的是 `http://localhost:8765/`。**首次在真实 Office 里旁加载时,验证 Office 是否接受 http://localhost**:

- ✅ 能加载任务窗格 → 走通,零证书。
- ❌ Office 拒绝 http(要求 https) → 按顺序尝试:

1. **桥加 HTTPS**:manifest 改为 `https://localhost:8765/`,桥监听处加 TLS(自签 localhost 证书,一次性装入系统受信任根)。改造点:`bridge.rs` 的 `Server::http` → `Server::new_ssl`(需要 openssl 特性)或用独立 TLS 终止。
2. **任务窗格打包进插件**:把 `shared/addin-web/dist` 拷贝到本目录本地旁加载,桥只提供 API(`http://127.0.0.1:8765/api/*`);桥需给 API 响应加 `Access-Control-Allow-Origin: *`,token 改为一次性配对(用户在 DocVault 设置中复制 token 粘贴进插件设置)。

## 常见问题

| 现象 | 原因 / 处理 |
|---|---|
| 窗格显示「DocVault 未运行或尚未打开仓库」 | 桌面端未启动或未连接仓库;先启动并连接。 |
| 窗格加载空白/无法访问 | `shared/addin-web/dist` 未构建;`npm run build` 后重启 DocVault。 |
| 保存提示「文档超过 20MB」 | Office.js 上限,改用「添加文档」手动导入。 |
