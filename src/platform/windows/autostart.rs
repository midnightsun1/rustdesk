//! Startup-folder autostart for the tray process.
//!
//! The installer drops `{app_name} Tray.lnk` into the all-users Startup folder
//! unconditionally. This module makes that shortcut a user-controlled switch:
//! `start-on-boot` says whether it should exist, and the tray process
//! reconciles the folder with that option. The main process cannot: its own
//! launch is already in flight, so it never observes the install sync.

use super::installer_shell::shortcut_bytes;
use hbb_common::{allow_err, config::LocalConfig, log, ResultType};
use std::path::PathBuf;
use windows::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};

const SHORTCUT_SUFFIX: &str = " Tray.lnk";
const STARTUP_DIR: &str = "Startup";
const TRAY_ARG: &str = "--tray";

pub fn check_autostart_config() {
    let enabled = LocalConfig::get_bool_option(base::config::keys::OPTION_START_ON_BOOT);
    allow_err!(set_enabled_impl(enabled));
}

/// Runs in the tray process, which outlives the install's shortcut sync.
/// The tray is started right after an install, before the desktop user's
/// config exists, so wait before trusting an empty option as "disabled".
/// Auto-connect also runs here: the Startup shortcut launches `--tray`, and
/// the tray has to open the connection window.
pub fn check_autostart_config_after_install_sync() {
    let start = std::time::Instant::now();
    while LocalConfig::get_option(base::config::keys::OPTION_START_ON_BOOT).is_empty() {
        if start.elapsed() >= std::time::Duration::from_secs(60)
            || !super::is_self_service_running()
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    check_autostart_config();
    connect_after_restart();
}

/// Launches the saved peer once on the launch that follows a restart.
/// Returns whether a connection window was launched.
pub fn connect_after_restart() -> bool {
    let Some(peer_id) = take_auto_connect_peer() else {
        return false;
    };
    match crate::run_me(vec!["--connect", &peer_id]) {
        Ok(_) => true,
        Err(err) => {
            log::error!("auto connect failed: {err}");
            false
        }
    }
}

pub fn is_enabled() -> bool {
    LocalConfig::get_bool_option(base::config::keys::OPTION_START_ON_BOOT)
}

/// Peer id to connect to on this launch, cleared so it runs once.
pub fn take_auto_connect_peer() -> Option<String> {
    let pending = LocalConfig::get_bool_option(base::config::keys::OPTION_AUTO_CONNECT_PENDING);
    let id = LocalConfig::get_option(base::config::keys::OPTION_AUTO_CONNECT_PEER_ID);
    if !pending || id.is_empty() {
        return None;
    }
    LocalConfig::set_option(
        base::config::keys::OPTION_AUTO_CONNECT_PENDING.to_owned(),
        String::new(),
    );
    Some(id)
}

pub fn set_enabled(enabled: bool) -> ResultType<()> {
    LocalConfig::set_option(
        base::config::keys::OPTION_START_ON_BOOT.to_owned(),
        if enabled { "Y" } else { "" }.to_owned(),
    );
    set_enabled_impl(enabled)
}

fn set_enabled_impl(enabled: bool) -> ResultType<()> {
    let path = shortcut_path()?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        };
    }
    if path.exists() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let bytes = shortcut_bytes(&exe.to_string_lossy(), Some(TRAY_ARG), None)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, bytes)?;
    log::info!("created startup shortcut {:?}", path);
    Ok(())
}

fn shortcut_path() -> ResultType<PathBuf> {
    Ok(common_programs_dir()?
        .join(STARTUP_DIR)
        .join(format!("{}{SHORTCUT_SUFFIX}", crate::get_app_name())))
}

fn common_programs_dir() -> ResultType<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_CommonPrograms, KF_FLAG_DEFAULT, None) }?;
    Ok(PathBuf::from(unsafe { path.to_string() }?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_path_is_in_the_startup_folder() {
        let path = shortcut_path().expect("Startup folder should resolve");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{}{}", crate::get_app_name(), SHORTCUT_SUFFIX)
        );
        assert!(path.parent().unwrap().ends_with(STARTUP_DIR));
    }
}