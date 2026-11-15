//! Mouse Settings: an offline settings app for gaming mice. Currently supports the Logitech G502 HERO.
//!
//! It edits the mouse's onboard profiles over HID++ 2.0. Every save starts from the bytes
//! on the mouse, changes only what the user changed, backs the old profile up first, and
//! reads the result back. No network, no background service.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod device;
mod error;
mod hidpp;
#[cfg(test)]
mod mock;
mod profile;
#[cfg(test)]
mod tests;
mod worker;

use std::path::PathBuf;

use serde::Serialize;
use tauri::{Manager, State};

use device::{MouseInfo, WriteReport};
use error::Error;
use profile::Settings;
use worker::Worker;

struct App {
    worker: Worker,
    backups: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileView {
    address: u16,
    settings: Settings,
    crc_ok: bool,
    /// The raw 256 bytes, for the "show raw bytes" panel.
    raw: Vec<u8>,
}

#[tauri::command]
async fn mouse_info(app: State<'_, App>) -> Result<MouseInfo, String> {
    Ok(app.worker.run(|m| m.info()).await?)
}

#[tauri::command]
async fn read_profile(app: State<'_, App>, address: u16) -> Result<ProfileView, String> {
    let image = app.worker.run(move |m| m.read_profile(address)).await?;
    Ok(ProfileView {
        address,
        settings: image.settings(),
        crc_ok: image.crc_ok(),
        raw: image.bytes().to_vec(),
    })
}

#[tauri::command]
async fn save_profile(
    app: State<'_, App>,
    address: u16,
    base: Settings,
    settings: Settings,
) -> Result<WriteReport, String> {
    let dir = app.backups.clone();
    Ok(app
        .worker
        .run(move |m| m.save_settings(address, &base, &settings, |old| backup::save(&dir, address, old)))
        .await?)
}

#[tauri::command]
async fn set_onboard_mode(app: State<'_, App>, onboard: bool) -> Result<(), String> {
    Ok(app.worker.run(move |m| m.set_mode(onboard)).await?)
}

#[tauri::command]
fn list_backups(app: State<'_, App>) -> Vec<backup::BackupEntry> {
    backup::list(&app.backups)
}

#[tauri::command]
async fn restore_backup(app: State<'_, App>, file: String) -> Result<WriteReport, String> {
    let dir = app.backups.clone();
    let (address, image) = backup::load(&dir, &file)?;
    Ok(app
        .worker
        .run(move |m| m.write_profile(address, &image, |old| backup::save(&dir, address, old)))
        .await?)
}

fn open_mouse() -> Result<Box<dyn hidpp::Transport>, Error> {
    hidpp::transport::open_g502()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let backups = app.path().app_data_dir()?.join("backups");
            app.manage(App { worker: Worker::spawn(open_mouse), backups });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            mouse_info,
            read_profile,
            save_profile,
            set_onboard_mode,
            list_backups,
            restore_backup,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Mouse Settings");
}
