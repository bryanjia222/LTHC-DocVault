# docvault-addin —— Office 插件安装/卸载工具

Windows 专用 Rust CLI,自动安装/卸载 DocVault 的 Office 插件(Word/Excel/PPT)。
基于 Office 官方"共享目录目录清单"(shared folder catalog)机制,`net share` 建共享、
写 `HKCU\...\WEF\TrustedCatalogs` 注册表,**共享目录对用户完全透明**。

## 构建

```bash
cd apps/office-addin-cli
cargo build --release   # → target/release/docvault-addin.exe
```

## 用法

```bash
# 安装(首次弹一次 UAC,工具自动以管理员重跑)。
# manifest 会自动从当前目录/可执行文件位置向上查找仓库内的
# apps/office-addin/manifest.xml —— 在仓库内任意目录运行即可。
docvault-addin install
docvault-addin install --restart          # 同时强制关闭 Office 应用立即生效(未保存文档会丢失)
docvault-addin install --manifest <path>  # 指定 manifest 绝对路径(仓库外运行时用)

# 卸载(删注册表条目)
docvault-addin uninstall
docvault-addin uninstall --remove-share   # 同时删除 DocVaultAddins 共享(需管理员)
```

## 它做了什么

**安装**:
1. 把 manifest.xml 拷到 `%LOCALAPPDATA%\DocVault\office-catalog`
2. `net share DocVaultAddins="<目录>" /grant:everyone,READ`(需管理员,自动重提权)
3. 写 `HKCU\Software\Microsoft\Office\16.0\WEF\TrustedCatalogs\{GUID}`:`Id` / `Url=\\<机器名>\DocVaultAddins` / `Flags=1`
4. (可选)关闭 Word/Excel/PPT;重启后插件出现在"插入 → 加载项 → 共享文件夹"。

**卸载**:删注册表键;`--remove-share` 时删共享。重启 Office 后移除。

## 原理

Office 持久安装非 M365 集中部署的插件,唯一可脚本化通道就是共享目录目录清单,
且官方要求目录是**网络路径**(本地路径不可靠)。任务窗格由 DocVault 本地桥
(localhost)托管,与需要 HTTPS 的集中部署不兼容,故走本通道。局域网分发:把
`apps/office-addin/manifest.xml` 放进共享,其他机器手动把同样的注册表 `Url`
指向 `\\<本机>\DocVaultAddins` 即可。

## 注意

- 需要 Office 16.0(2016+/Microsoft 365)桌面版。
- `--restart` 用 `taskkill /F` 强杀 Office,会丢失未保存文档,慎用。
