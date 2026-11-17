//! Startup-folder autostart for the tray process.
//!
//! The installer drops `{app_name} Tray.lnk` into the all-users Startup folder
//! unconditionally. This module makes that shortcut a user-controlled switch:
//! `start-on-boot` says whether it should exist, and every process start
//! reconciles the folder with that option.

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