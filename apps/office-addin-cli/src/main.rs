//! docvault-addin —— 安装 / 卸载 DocVault 的 Office 插件。
//!
//! Office 在 Windows 上持久安装 Word/Excel/PPT 插件的正规通道是"共享目录目录清单"
//! (shared folder catalog):把一个含 manifest.xml 的目录注册为受信任目录,
//! 注册表位置 `HKCU\Software\Microsoft\Office\16.0\WEF\TrustedCatalogs\{GUID}`。
//! 官方要求目录必须是**网络路径**(本地路径不可靠),所以本工具替你完成全部动作,
//! "共享目录"对用户透明:
//!
//!   1. 把 manifest 拷进 `%LOCALAPPDATA%\DocVault\office-catalog`
//!   2. 用 `net share` 把它共享为 `DocVaultAddins`(需一次管理员提权,工具自动重提)
//!   3. 写注册表指向 `\\<机器名>\DocVaultAddins`
//!   4. (可选)关闭 Office,提示用户重启生效
//!
//! 卸载 = 删注册表键 + (可选)删共享。整个流程无需用户手动共享任何目录。

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use winreg::enums::*;
use winreg::RegKey;

/// 固定的目录目录清单 GUID(与插件 Id 无关),保证 install/uninstall 幂等匹配。
const CATALOG_GUID: &str = "3f7a1c9e-5b2d-4e8a-9c4f-6d1b0a2e7c35";
const SHARE_NAME: &str = "DocVaultAddins";
const REG_BASE: &str = r"Software\Microsoft\Office\16.0\WEF\TrustedCatalogs";
const OFFICE_EXES: &[&str] = &["WINWORD.EXE", "EXCEL.EXE", "POWERPNT.EXE"];

#[derive(Parser)]
#[command(name = "docvault-addin", about = "安装/卸载 DocVault 的 Office 插件(Word/Excel/PPT)")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// 安装(注册)插件。首次需管理员权限以创建共享目录。
    Install {
        /// manifest.xml 路径(相对当前目录)
        #[arg(long, default_value = "apps/office-addin/manifest.xml")]
        manifest: PathBuf,
        /// 同时强制关闭运行中的 Office 应用使插件立即生效(未保存文档会丢失)
        #[arg(long)]
        restart: bool,
    },
    /// 卸载(注销)插件。
    Uninstall {
        /// 同时删除 DocVaultAddins 共享(需管理员权限)
        #[arg(long)]
        remove_share: bool,
    },
}

fn main() -> Result<()> {
    // 保存原始参数,供"非管理员→自动重提权"时原样传给提权后的实例。
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let cli = Cli::parse();
    match cli.action {
        Action::Install { manifest, restart } => install(&manifest, restart, &raw_args)?,
        Action::Uninstall { remove_share } => uninstall(remove_share, &raw_args)?,
    }
    Ok(())
}

/// 是否为管理员进程:`net session` 对非管理员返回访问拒绝。
fn is_admin() -> bool {
    Command::new("net")
        .arg("session")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// 通过 PowerShell `Start-Process -Verb RunAs` 以管理员身份重新运行本工具,
/// 参数原样传递(触发一次 UAC 提示)。
fn relaunch_elevated(args: &[String]) -> Result<()> {
    let exe = env::current_exe().context("定位当前可执行文件")?;
    let quoted = args
        .iter()
        .map(|a| format!(r#""{a}""#))
        .collect::<Vec<_>>()
        .join(" ");
    let ps = format!(
        r#"Start-Process -Verb RunAs -FilePath "{}" -ArgumentList {quoted}"#,
        exe.display()
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .status()
        .context("调用 PowerShell 重提权")?;
    if !status.success() {
        bail!("UAC 提权被取消或失败");
    }
    Ok(())
}

fn install(manifest: &Path, restart: bool, raw_args: &[String]) -> Result<()> {
    if !manifest.exists() {
        bail!("manifest 不存在: {}", manifest.display());
    }
    if !is_admin() {
        println!("创建共享目录需要管理员权限,正在以管理员身份重新运行…");
        relaunch_elevated(raw_args)?;
        return Ok(());
    }

    let localappdata = env::var("LOCALAPPDATA").context("缺少 LOCALAPPDATA 环境变量")?;
    let catalog_dir = Path::new(&localappdata).join("DocVault").join("office-catalog");
    std::fs::create_dir_all(&catalog_dir).context("创建目录清单目录")?;
    std::fs::copy(manifest, catalog_dir.join("manifest.xml"))
        .context("拷贝 manifest 到目录清单目录")?;

    ensure_share(&catalog_dir)?;

    let computer = env::var("COMPUTERNAME").unwrap_or_else(|_| "localhost".to_owned());
    let unc = format!(r"\\{computer}\{SHARE_NAME}");
    register_catalog(&unc)?;

    println!("已安装 DocVault Office 插件:");
    println!("  目录清单目录 : {}", catalog_dir.display());
    println!("  共享路径     : {unc}");
    println!("  注册表       : {REG_BASE}\\{{{CATALOG_GUID}}}");
    if restart {
        println!("正在关闭运行中的 Office 应用(未保存的文档会丢失)…");
        close_office();
        println!("已关闭。重新打开 Word/Excel/PowerPoint 后插件生效。");
    } else {
        println!("请关闭并重新打开 Word/Excel/PowerPoint 使插件生效。");
    }
    Ok(())
}

fn uninstall(remove_share: bool, raw_args: &[String]) -> Result<()> {
    if remove_share && !is_admin() {
        println!("删除共享需要管理员权限,正在以管理员身份重新运行…");
        relaunch_elevated(raw_args)?;
        return Ok(());
    }
    unregister_catalog()?;
    if remove_share {
        let _ = Command::new("net").args(["share", SHARE_NAME, "/delete"]).output();
        println!("已删除共享 {SHARE_NAME}。");
    }
    println!("已卸载。请关闭并重新打开 Office 应用以移除插件。");
    Ok(())
}

/// 把目录清单目录共享为 `DocVaultAddins`(先删后建,幂等)。读取权限授予 Everyone,
/// 便于同一台机器上的 Office 与本机访问;局域网分发时,其他机器把同样的注册表
/// `Url` 指向 `\\<本机>\DocVaultAddins` 即可(本工具暂未内置该模式)。
fn ensure_share(catalog_dir: &Path) -> Result<()> {
    let _ = Command::new("net").args(["share", SHARE_NAME, "/delete"]).output();
    let share_arg = format!("{SHARE_NAME}={}", catalog_dir.display());
    let out = Command::new("net")
        .args(["share", &share_arg, "/grant:everyone,READ"])
        .output()
        .context("执行 net share")?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stdout);
        bail!("创建共享失败: {msg}");
    }
    Ok(())
}

fn register_catalog(unc: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"{REG_BASE}\{{{CATALOG_GUID}}}");
    let (key, _) = hkcu.create_subkey(&key_path).context("创建注册表键")?;
    key.set_value("Id", &format!("{{{CATALOG_GUID}}}"))
        .context("写 Id")?;
    key.set_value("Url", &unc).context("写 Url")?;
    key.set_value("Flags", &1u32).context("写 Flags")?;
    Ok(())
}

fn unregister_catalog() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key_path = format!(r"{REG_BASE}\{{{CATALOG_GUID}}}");
    match hkcu.delete_subkey_all(&key_path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), // 未安装过,视为成功
        Err(e) => Err(e).context("删除注册表键"),
    }
}

fn close_office() {
    for exe in OFFICE_EXES {
        let _ = Command::new("taskkill").args(["/IM", exe, "/F"]).output();
    }
}
