# @docvault/addin-web

共享任务窗格 UI + DocVault 本地桥客户端,供 Office / WPS 插件使用。纯 TypeScript + Vite,**不引框架**,保持任务窗格轻量。

## 目录

```
src/
  bridge.ts       桥 API 客户端(health / listDocuments / import / commitVersion)
  host.ts         HostAdapter 接口 + 20MB 上限常量 + TooLargeError
  hosts/office.ts Office.js 适配(getFileAsync 分片读当前文档)
  hosts/wps.ts    WPS 适配(二期:SaveAs 到临时路径,免 20MB 上限)
  hosts/index.ts  detectHost() 按宿主全局对象选适配器
  taskpane.ts     任务窗格装配:健康检查 → 目标选择器(新增/提交新版本)→ 保存
  main.ts         入口(detectHost + mountTaskPane)
```

## 工作原理

- 任务窗格页面由 **DocVault 桌面端内嵌的 localhost 桥**(`127.0.0.1:8765`)托管,与 API 同源。
- 桥在服务端把会话 token 注入为 `window.__DOCVAULT_TOKEN__`,`bridge.ts` 自动带上 `Authorization: Bearer`。
- 保存流程:宿主适配器读当前文档(`getFileAsync` 分片,超 20MB 抛 `TooLargeError` → 提示手动导入)→ 按所选模式 POST `/api/documents/import` 或 `/api/documents/{id}/versions` → 桥落盘后走与桌面端相同的两阶段提交(Phase A + Archive 作业),桌面端经现有 `job:update` 事件自动刷新文档列表。

## 构建

```bash
npm install
npm run build   # tsc --noEmit && vite build → dist/
```

构建产物由桥在 `GET /` 托管(注入 token)。纯浏览器调试页可用 `npm run dev`(此时无宿主,`detectHost` 会报"未检测到宿主")。
