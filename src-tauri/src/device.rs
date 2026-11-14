//! What the app can do with the mouse, built on HID++ 2.0 features:
//! `0x0000` Root, `0x0003` firmware info, `0x0005` device name, `0x8100` onboard profiles.
//!
//! `0x8100` functions used (parameters big-endian, no memory-type byte, as in Solaar and libratbag):
//! - 0 getDescription → memModel, profileFormat, macroFormat, profileCount, profileCountOOB,
//!   buttonCount, sectorCount, sectorSize (u16), …
//! - 1 setMode `[mode]`, 2 getMode → `[mode]` (1 = onboard, 2 = host)
//! - 3 setCurrentProfile `[addr_hi, addr_lo]`, 4 getCurrentProfile → `[addr_hi, addr_lo]`
//! - 5 memoryRead `[sector_hi, sector_lo, offset_hi, offset_lo]` → 16 bytes
//! - 6 memoryWriteStart `[sector_hi, sector_lo, offset_hi, offset_lo, len_hi, len_lo]`
//! - 7 memoryWrite `[16 bytes]`, 8 memoryWriteEnd

use serde::Serialize;

use crate::error::Error;
use crate::hidpp::{Hidpp, Transport, FEATURE_ROOT, PARAMS_LEN};
use crate::profile::{crc16, ProfileImage, Settings, PROFILE_LEN};

pub const FEATURE_FW_INFO: u16 = 0x0003;
pub const FEATURE_NAME: u16 = 0x0005;
pub const FEATURE_ONBOARD: u16 = 0x8100;

/// Sector 0 holds the profile directory; it is read, never written.
pub const DIRECTORY_SECTOR: u16 = 0x0000;
/// Addresses from 0x0100 up are factory (ROM) profiles: readable, never writable.
pub const ROM_BASE: u16 = 0x0100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// The mouse runs from its own memory (what this app edits).
    Onboard,
    /// Software (G Hub) is driving the mouse; onboard profiles are ignored.
    Host,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    pub profile_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub address: u16,
    pub enabled: bool,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MouseInfo {
    pub name: String,
    pub firmware: Option<String>,
    pub protocol: String,
    pub mode: Mode,
    pub description: Description,
    pub slots: Vec<Slot>,
    pub current_profile: u16,
}

pub struct Mouse<T: Transport> {
    pub hidpp: Hidpp<T>,
}

/// Parses the sector-0 directory: 4-byte entries `[addr_hi, addr_lo, enabled, _]` up to `FF FF`.
pub fn parse_directory(bytes: &[u8]) -> Vec<Slot> {
    bytes
        .chunks_exact(4)
        .map(|e| (u16::from_be_bytes([e[0], e[1]]), e[2] != 0))
        .take_while(|&(addr, _)| addr != 0xFFFF)
        .map(|(address, enabled)| Slot { address, enabled, writable: address != 0 && address < ROM_BASE })
        .collect()
}

impl<T: Transport> Mouse<T> {
    pub fn new(transport: T) -> Self {
        Mouse { hidpp: Hidpp::new(transport) }
    }

    pub fn protocol_version(&mut self) -> Result<String, Error> {
        let r = self.hidpp.call(FEATURE_ROOT, 1, &[0, 0, 0x5A])?;
        Ok(format!("{}.{}", r[0], r[1]))
    }

    pub fn name(&mut self) -> Result<String, Error> {
        let len = self.hidpp.call(FEATURE_NAME, 0, &[])?[0] as usize;
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let chunk = self.hidpp.call(FEATURE_NAME, 1, &[out.len() as u8])?;
            let take = (len - out.len()).min(PARAMS_LEN);
            out.extend_from_slice(&chunk[..take]);
        }
        Ok(String::from_utf8_lossy(&out).trim_end_matches('\0').trim().to_string())
    }

    /// Main firmware version, e.g. `U1 27.03.B0016` (number and revision are BCD).
    pub fn firmware(&mut self) -> Result<String, Error> {
        let count = self.hidpp.call(FEATURE_FW_INFO, 0, &[])?[0];
        for entity in 0..count {
            let r = self.hidpp.call(FEATURE_FW_INFO, 1, &[entity])?;
            if r[0] & 0x0F == 0 {
                let prefix: String = r[1..4].iter().map(|&c| c as char).collect();
                let build = u16::from_be_bytes([r[6], r[7]]);
                return Ok(format!("{} {:02X}.{:02X}.B{:04X}", prefix.trim(), r[4], r[5], build));
            }
        }
        Err(Error::Invalid("The mouse reported no main firmware".into()))
    }

    pub fn description(&mut self) -> Result<Description, Error> {
        let r = self.hidpp.call(FEATURE_ONBOARD, 0, &[])?;
        Ok(Description {
            profile_count: r[3],
            button_count: r[5],
            sector_count: r[6],
            sector_size: u16::from_be_bytes([r[7], r[8]]),
        })
    }

    pub fn mode(&mut self) -> Result<Mode, Error> {
        Ok(match self.hidpp.call(FEATURE_ONBOARD, 2, &[])?[0] {
            1 => Mode::Onboard,
            2 => Mode::Host,
            _ => Mode::Unknown,
        })
    }

    pub fn set_mode(&mut self, onboard: bool) -> Result<(), Error> {
        self.hidpp.call(FEATURE_ONBOARD, 1, &[if onboard { 1 } else { 2 }])?;
        Ok(())
    }

    pub fn current_profile(&mut self) -> Result<u16, Error> {
        let r = self.hidpp.call(FEATURE_ONBOARD, 4, &[])?;
        Ok(u16::from_be_bytes([r[0], r[1]]))
    }

    pub fn set_current_profile(&mut self, address: u16) -> Result<(), Error> {
        self.hidpp.call(FEATURE_ONBOARD, 3, &address.to_be_bytes())?;
        Ok(())
    }

    /// Reads `len` bytes of a sector, 16 at a time. A last partial chunk is read at
    /// `len - 16` because the mouse refuses reads that run past the sector end.
    pub fn read_sector(&mut self, sector: u16, len: usize) -> Result<Vec<u8>, Error> {
        if len < PARAMS_LEN {
            return Err(Error::Invalid(format!("Sector length {len} is too small")));
        }
        let [s_hi, s_lo] = sector.to_be_bytes();
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let offset = out.len().min(len - PARAMS_LEN);
            let skip = out.len() - offset;
            let [o_hi, o_lo] = (offset as u16).to_be_bytes();
            let chunk = self.hidpp.call(FEATURE_ONBOARD, 5, &[s_hi, s_lo, o_hi, o_lo])?;
            out.extend_from_slice(&chunk[skip..]);
        }
        Ok(out)
    }

    fn write_sector(&mut self, sector: u16, bytes: &[u8]) -> Result<(), Error> {
        if sector == DIRECTORY_SECTOR || sector >= ROM_BASE {
            return Err(Error::Refused(format!("sector {sector:#06X} is never written by this app")));
        }
        if bytes.len() % PARAMS_LEN != 0 {
            return Err(Error::Invalid("Sector data must be a multiple of 16 bytes".into()));
        }
        let [s_hi, s_lo] = sector.to_be_bytes();
        let [l_hi, l_lo] = (bytes.len() as u16).to_be_bytes();
        self.hidpp.call(FEATURE_ONBOARD, 6, &[s_hi, s_lo, 0, 0, l_hi, l_lo])?;
        for chunk in bytes.chunks_exact(PARAMS_LEN) {
            self.hidpp.call(FEATURE_ONBOARD, 7, chunk)?;
        }
        self.hidpp.call(FEATURE_ONBOARD, 8, &[])?;
        Ok(())
    }

    pub fn info(&mut self) -> Result<MouseInfo, Error> {
        // Protocol first: it's the cheapest proof the connection works.
        let protocol = self.protocol_version()?;
        let description = self.description()?;
        let directory = self.read_sector(DIRECTORY_SECTOR, description.sector_size as usize)?;
        Ok(MouseInfo {
            name: self.name().unwrap_or_else(|_| "G502 HERO".into()),
            firmware: self.firmware().ok(),
            protocol,
            mode: self.mode()?,
            slots: parse_directory(&directory),
            current_profile: self.current_profile()?,
            description,
        })
    }

    /// Checks that `address` is a profile this app may write, and returns the description.
    fn check_writable(&mut self, address: u16) -> Result<Description, Error> {
        let description = self.description()?;
        if description.sector_size as usize != PROFILE_LEN {
            return Err(Error::Refused(format!(
                "the mouse uses {}-byte memory sectors and this app only handles {PROFILE_LEN}",
                description.sector_size
            )));
        }
        let directory = self.read_sector(DIRECTORY_SECTOR, PROFILE_LEN)?;
        // Like a profile, the directory ends in a CRC-CCITT (libratbag checks it too). If it
        // fails, the mouse ignores the directory and runs a factory profile: don't write.
        let stored = u16::from_be_bytes([directory[PROFILE_LEN - 2], directory[PROFILE_LEN - 1]]);
        if crc16(&directory[..PROFILE_LEN - 2]) != stored {
            return Err(Error::Refused(
                "the mouse's profile list fails its checksum. Set the mouse up once in Logitech G Hub".into(),
            ));
        }
        match parse_directory(&directory).into_iter().find(|s| s.address == address) {
            Some(s) if s.writable && s.enabled => Ok(description),
            Some(s) if !s.writable => Err(Error::Refused("factory profiles can't be changed".into())),
            Some(_) => Err(Error::Refused(format!("profile {address} is switched off on the mouse"))),
            None => Err(Error::Refused(format!("profile {address} isn't set up on the mouse"))),
        }
    }

    pub fn read_profile(&mut self, address: u16) -> Result<ProfileImage, Error> {
        let bytes = self.read_sector(address, PROFILE_LEN)?;
        ProfileImage::from_bytes(&bytes)
    }

    /// Writes `new` to `address` after the safety checks, then reads it back.
    ///
    /// `backup` is called with the profile currently on the mouse *before* anything is
    /// written; if it fails, nothing is written.
    pub fn write_profile(
        &mut self,
        address: u16,
        new: &ProfileImage,
        backup: impl FnOnce(&ProfileImage) -> Result<String, Error>,
    ) -> Result<WriteReport, Error> {
        if !new.crc_ok() {
            return Err(Error::Refused("the new profile's checksum is wrong".into()));
        }
        self.check_writable(address)?;
        let before = self.read_profile(address)?;
        let changed = new.diff(&before).len();
        if changed == 0 {
            return Ok(WriteReport { backup_file: None, bytes_changed: 0, reloaded: false });
        }
        let backup_file = backup(&before)?;

        // From here on the profile may be half-written, so every error names the backup.
        let after_write = |cause: Error| Error::AfterWrite { cause: Box::new(cause), backup: backup_file.clone() };
        self.write_sector(address, new.bytes()).map_err(after_write)?;
        let after = self.read_profile(address).map_err(after_write)?;
        if &after != new {
            return Err(after_write(Error::Invalid("The mouse saved something different from what was sent.".into())));
        }
        // Make the mouse pick up the new settings now if this is the profile in use. The
        // write already succeeded and was verified, so a failure here isn't reported as one.
        let reloaded = matches!(self.current_profile(), Ok(c) if c == address)
            && self.set_current_profile(address).is_ok();
        Ok(WriteReport { backup_file: Some(backup_file), bytes_changed: changed, reloaded })
    }

    /// Applies the page's edits to the profile on the mouse and writes it.
    ///
    /// `base` is the settings the page loaded and `wanted` the edited ones. If the mouse's
    /// profile no longer matches `base` (changed by G Hub or a button since), nothing is
    /// written: otherwise fields the user never touched could be overwritten with stale values.
    pub fn save_settings(
        &mut self,
        address: u16,
        base: &Settings,
        wanted: &Settings,
        backup: impl FnOnce(&ProfileImage) -> Result<String, Error>,
    ) -> Result<WriteReport, Error> {
        let current = self.read_profile(address)?;
        if &current.settings() != base {
            return Err(Error::Refused(
                "the profile on the mouse changed since it was loaded. Press Refresh and make your changes again".into(),
            ));
        }
        let new = current.with_settings(wanted)?;
        self.write_profile(address, &new, backup)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub backup_file: Option<String>,
    pub bytes_changed: usize,
    pub reloaded: bool,
}
