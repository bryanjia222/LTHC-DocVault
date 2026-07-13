# Plan: Add-Document dialog + Switch-Backend dialog

Frontend-only feature. `commit_document` already accepts `author`, and
`connect_vault` + its structured `ConnectError` already exist — no Rust changes.

## Goals
1. **Add document** → a modal dialog with: file picker, document name
   (auto-filled from the file stem, editable), author (optional).
2. **Switch backend** → a button that opens a modal dialog with the existing
   connect fields (dir, backend, password), replacing the inline Settings form.

## Reuse / architecture
- Reuse the **CommandPalette overlay pattern** (`<Teleport to="body">` + backdrop
  + `role="dialog" aria-modal="true"`, Esc/backdrop close) as a shared
  `BaseModal.vue` so both dialogs don't duplicate it.
- A small **`useDialogs` singleton composable** (module-level `ref`s) holds
  open-state, so `runAction('actionLogs.addDocument')` (in `useVaultActions`)
  and a Settings button can open dialogs that are mounted once at app level.
- Extract the file helpers (`pickOfficeFile`, `deriveNameFromPath`, `extOf`)
  from `useVaultActions` into `utils/file.ts` so the add-document dialog reuses
  them (no duplication).

## New files
1. `src/utils/file.ts` — `pickOfficeFile()`, `deriveNameFromPath()`, `extOf()`
   (moved out of `useVaultActions`, which re-imports them).
2. `src/composables/useDialogs.ts` — `addDocumentOpen`, `switchBackendOpen`
   refs + `open/close` fns for each.
3. `src/components/BaseModal.vue` — props `open`, `title`, `subtitle?`;
   emits `close`; slots `default` (body) + `footer`; Esc + backdrop + X button
   close; locks body scroll while open.
4. `src/components/AddDocumentDialog.vue` — fields:
   - File: browse button (`pickOfficeFile`) → shows selected filename; on pick,
     auto-fill name = file stem.
   - Name: text input, editable (auto-filled, user can change).
   - Author: text input, optional.
   - Submit (disabled until a file is picked) →
     `commit({ path, new_name: name.trim() || stem, author: author.trim() || undefined })`.
   - Logging: `actionRequested` on open; `jobStarted`/`actionFailed` on submit;
     `actionCancelled` on close-without-submit (tracked via a `submitted` flag).
   - State reset on open.
5. `src/components/SwitchBackendDialog.vue` — moves SettingsView's switch form
   into the dialog: dir (browse), backend select (pre-filled to current
   `config.backend`), password (restic only), submit → `connect(...)`.
   - Error mapping (`ConnectError` kind → localized) preserved.
   - On success: show status, clear password, keep open so the user sees
     confirmation (Close dismisses). On error: stay open with the message.

## Modified files
6. `src/composables/useVaultActions.ts` —
   `runAction('actionLogs.addDocument')` → `openAddDocument()` (from
   `useDialogs`); remove the old immediate pick+commit `addDocumentAction`;
   import file helpers from `utils/file`.
7. `src/components/views/SettingsView.vue` — replace the `switch-card` form with
   a single "Switch vault / backend" button → `openSwitchBackend()`; delete the
   switch state + `pickDir`/`doConnect` logic (now in the dialog).
8. `src/App.vue` — mount `<AddDocumentDialog />` + `<SwitchBackendDialog />`
   alongside `<CommandPalette />`.
9. `src/i18n/locales/en-US.ts` + `zh-CN.ts` — add:
   - `dialog.close` ("Close" / "关闭")
   - `addDocument.{title, fileLabel, filePlaceholder, browse, nameLabel,
     namePlaceholder, authorLabel, authorPlaceholder, submit, noFile}`
   - `connect.title` ("Switch vault / backend" / "切换仓库 / 后端")
   - Reuse existing `connect.*`, `backend.*`, `actions.cancel`.

## Non-goals
- No new command-palette entry (add-document stays reachable from Documents).
- No topbar/sidebar placement for switch-backend (stays in Settings as a button).
- No browser-dev file picking (Tauri-only, matching existing `isTauri` guards).

## Verification (frontend-only, no Rust touched)
- `cd apps/desktop && npm run lint`
- `cd apps/desktop && npm run build` (vue-tsc type-check + vite)
- Spot-check: add-document dialog auto-fills name from a picked file, author
  flows into the commit job; switch-backend dialog shows errors inline and
  updates the sidebar/backend label on success.
