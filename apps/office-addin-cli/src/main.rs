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
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use clap::error::ErrorKind;
use clap::{Parser, Subcommand};
use tracing::error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use winreg::enums::*;
use winreg::RegKey;

/// 固定的目录目录清单 GUID(与插件 Id 无关),保证 install/uninstall 幂等匹配。
const CATALOG_GUID: &str = "3f7a1c9e-5b2d-4e8a-9c4f-6d1b0a2e7c35";
const SHARE_NAME: &str = "DocVaultAddins";
const REG_BASE: &str = r"Software\Microsoft\Office\16.0\WEF\TrustedCatalogs";
const OFFICE_EXES: &[&str] = &["WINWORD.EXE", "EXCEL.EXE", "POWERPNT.EXE"];

#[derive(Parser)]
#[command(
    name = "docvault-addin",
    about = "安装/卸载 DocVault 的 Office 插件(Word/Excel/PPT)"
)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// 安装(注册)插件。首次需管理员权限以创建共享目录。
    Install {
        /// manifest.xml 路径(默认:从当前目录/可执行文件位置向上查找仓库内
        /// apps/office-addin/manifest.xml,因此可在仓库内任意目录运行)
        #[arg(long)]
        manifest: Option<PathBuf>,
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

fn main() -> ExitCode {
    // 保存原始参数,供"非管理员→自动重提权"时原样传给提权后的实例。
    let raw_args: Vec<String> = env::args().skip(1).collect();
    let _guard = init_logging();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(parse_error)
            if matches!(
                parse_error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            return match parse_error.print() {
                Ok(()) => ExitCode::SUCCESS,
                Err(print_error) => {
                    error!(error = %print_error, "failed to print add-in diagnostic");
                    eprintln!("{print_error}");
                    ExitCode::from(2)
                }
            };
        }
        Err(parse_error) => {
            error!(error = %parse_error, "office add-in command-line parse failed");
            eprintln!("{parse_error}");
            return ExitCode::from(2);
        }
    };
    match run_action(&raw_args, cli.action) {
        Ok(()) => ExitCode::SUCCESS,
        Err(command_error) => {
            error!(error = %command_error, "office add-in command failed");
            eprintln!("{command_error}");
            ExitCode::from(2)
        }
    }
}

fn run_action(raw_args: &[String], action: Action) -> Result<()> {
    match action {
        Action::Install { manifest, restart } => {
            let manifest = resolve_manifest(manifest.as_deref())?;
            install(&manifest, restart, raw_args)
        }
        Action::Uninstall { remove_share } => uninstall(remove_share, raw_args),
    }
}

fn init_logging() -> Option<WorkerGuard> {
    let Some(local_app_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        eprintln!("warning: could not determine the log directory; logging to stderr only");
        return None;
    };
    let log_dir = local_app_data.join("DocVault").join("logs");
    if let Err(create_error) = std::fs::create_dir_all(&log_dir) {
        eprintln!(
            "warning: could not create add-in log directory ({}): {create_error}",
            log_dir.display()
        );
        return None;
    }

    let file_appender = tracing_appender::rolling::daily(&log_dir, "docvault-addin.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    if let Err(init_error) = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::WARN)
        .with_writer(non_blocking)
        .with_target(false)
        .try_init()
    {
        eprintln!("warning: could not install add-in file logger: {init_error}");
    }
    Some(guard)
}

/// 定位 manifest.xml:`--manifest` 显式指定则校验;否则从当前目录与可执行文件
/// 所在目录逐级向上找 `apps/office-addin/manifest.xml`,使工具可在仓库内任意
/// 位置运行(包括 target/release)。找不到给出明确指引。
fn resolve_manifest(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        bail!("--manifest 不存在: {}", path.display());
    }
    let cwd = env::current_dir().ok();
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()));
    for start in [cwd, exe_dir].into_iter().flatten() {
        let mut dir = start;
        loop {
            let candidate = dir.join("apps").join("office-addin").join("manifest.xml");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    bail!("未找到 manifest.xml。请在仓库目录内运行本工具,或用 --manifest 指定其绝对路径。")
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
    println!("manifest : {}", manifest.display());
    if !manifest.exists() {
        bail!("manifest 不存在: {}", manifest.display());
    }
    if !is_admin() {
        println!("创建共享目录需要管理员权限,正在以管理员身份重新运行…");
        relaunch_elevated(raw_args)?;
        return Ok(());
    }

    let localappdata = env::var("LOCALAPPDATA").context("缺少 LOCALAPPDATA 环境变量")?;
    let catalog_dir = Path::new(&localappdata)
        .join("DocVault")
        .join("office-catalog");
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
        let _ = Command::new("net")
            .args(["share", SHARE_NAME, "/delete"])
            .output();
        println!("已删除共享 {SHARE_NAME}。");
    }
    println!("已卸载。请关闭并重新打开 Office 应用以移除插件。");
    Ok(())
}

/// 把目录清单目录共享为 `DocVaultAddins`(先删后建,幂等)。读取权限授予 Everyone,
/// 便于同一台机器上的 Office 与本机访问;局域网分发时,其他机器把同样的注册表
/// `Url` 指向 `\\<本机>\DocVaultAddins` 即可(本工具暂未内置该模式)。
fn ensure_share(catalog_dir: &Path) -> Result<()> {
    let _ = Command::new("net")
        .args(["share", SHARE_NAME, "/delete"])
        .output();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_manifest_by_walking_up_from_crate_dir() {
        // 测试 CWD 是 crate 根(office-addin-cli),向上找应命中仓库内的 manifest。
        let resolved = resolve_manifest(None).expect("仓库内应能找到 manifest");
        assert!(
            resolved.ends_with(Path::new("apps/office-addin/manifest.xml")),
            "unexpected path: {resolved:?}"
        );
    }

    #[test]
    fn explicit_missing_manifest_errors() {
        assert!(
            resolve_manifest(Some(Path::new("definitely-not-here.xml"))).is_err(),
            "缺失的显式 --manifest 应报错"
        );
    }
}
