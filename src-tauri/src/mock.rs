//! A simulated G502 HERO for tests. It parses requests with the real byte layout and
//! answers bad layouts with HID++ errors, so a framing bug fails a test instead of a mouse.

use std::collections::{HashMap, VecDeque};

use crate::device::{FEATURE_FW_INFO, FEATURE_NAME, FEATURE_ONBOARD};
use crate::error::Error;
use crate::hidpp::{Report, Transport, DEVICE_WIRED, REPORT_ID_LONG, REPORT_LEN};
use crate::profile::{crc16, tests::sample, PROFILE_LEN};

const INDEX_FW: u8 = 1;
const INDEX_NAME: u8 = 2;
const INDEX_ONBOARD: u8 = 3;

pub struct MockMouse {
    pub sectors: HashMap<u16, Vec<u8>>,
    pub sector_size: u16,
    pub mode: u8,
    pub current: u16,
    /// Every sector a write was started on.
    pub writes_started: Vec<u16>,
    /// When set, memoryWriteEnd stores this instead of what was sent (simulates a bad write).
    pub corrupt_next_write: bool,
    pub reloads: usize,
    pub unplugged: bool,
    /// When set, memoryWriteEnd fails with a hardware error after storing nothing.
    pub fail_write_end: bool,
    write_target: Option<u16>,
    write_buf: Vec<u8>,
    outbox: VecDeque<Report>,
}

impl MockMouse {
    pub fn new() -> Self {
        let mut directory = vec![0xFF; PROFILE_LEN];
        directory[0..4].copy_from_slice(&[0x00, 0x01, 0x01, 0x00]); // profile 1, on
        directory[4..8].copy_from_slice(&[0x00, 0x02, 0x00, 0x00]); // profile 2, off
        directory[8..12].copy_from_slice(&[0x01, 0x01, 0x01, 0x00]); // ROM profile
        let crc = crc16(&directory[..PROFILE_LEN - 2]);
        directory[PROFILE_LEN - 2..].copy_from_slice(&crc.to_be_bytes());
        let mut sectors = HashMap::new();
        sectors.insert(0x0000, directory);
        sectors.insert(0x0001, sample().bytes().to_vec());
        sectors.insert(0x0002, sample().bytes().to_vec());
        sectors.insert(0x0101, sample().bytes().to_vec());
        MockMouse {
            sectors,
            sector_size: PROFILE_LEN as u16,
            mode: 1,
            current: 0x0001,
            writes_started: Vec::new(),
            corrupt_next_write: false,
            reloads: 0,
            unplugged: false,
            fail_write_end: false,
            write_target: None,
            write_buf: Vec::new(),
            outbox: VecDeque::new(),
        }
    }

    fn reply(&mut self, req: &Report, params: &[u8]) {
        let mut r = [0u8; REPORT_LEN];
        r[..4].copy_from_slice(&req[..4]);
        r[4..4 + params.len()].copy_from_slice(params);
        self.outbox.push_back(r);
    }

    fn error(&mut self, req: &Report, code: u8) {
        let mut r = [0u8; REPORT_LEN];
        r[0] = REPORT_ID_LONG;
        r[1] = DEVICE_WIRED;
        r[2] = 0xFF;
        r[3] = req[2];
        r[4] = req[3];
        r[5] = code;
        self.outbox.push_back(r);
    }
}

impl Transport for MockMouse {
    fn write(&mut self, req: &Report) -> Result<(), Error> {
        if self.unplugged {
            return Err(Error::Connection("device gone".into()));
        }
        // A button notification arrives before every reply, to exercise filtering.
        let mut noise = [0u8; REPORT_LEN];
        noise[0] = REPORT_ID_LONG;
        noise[1] = DEVICE_WIRED;
        noise[2] = INDEX_ONBOARD;
        noise[3] = 0x00;
        self.outbox.push_back(noise);

        let p = &req[4..];
        let u16at = |i: usize| u16::from_be_bytes([p[i], p[i + 1]]);
        match (req[2], req[3] >> 4) {
            (0, 0) => {
                let idx = match u16at(0) {
                    FEATURE_FW_INFO => INDEX_FW,
                    FEATURE_NAME => INDEX_NAME,
                    FEATURE_ONBOARD => INDEX_ONBOARD,
                    _ => 0,
                };
                self.reply(req, &[idx]);
            }
            (0, 1) => self.reply(req, &[4, 2, p[2]]),
            (INDEX_FW, 0) => self.reply(req, &[1]),
            (INDEX_FW, 1) => self.reply(req, &[0x00, b'U', b'1', b' ', 0x27, 0x03, 0x00, 0x16]),
            (INDEX_NAME, 0) => self.reply(req, &[9]),
            (INDEX_NAME, 1) => {
                let name = b"G502 HERO";
                let from = (p[0] as usize).min(name.len());
                self.reply(req, &name[from..]);
            }
            (INDEX_ONBOARD, 0) => {
                let [hi, lo] = self.sector_size.to_be_bytes();
                self.reply(req, &[1, 3, 1, 5, 1, 11, 16, hi, lo, 0]);
            }
            (INDEX_ONBOARD, 1) => {
                self.mode = p[0];
                self.reply(req, &[]);
            }
            (INDEX_ONBOARD, 2) => {
                let m = self.mode;
                self.reply(req, &[m]);
            }
            (INDEX_ONBOARD, 3) => {
                self.current = u16at(0);
                self.reloads += 1;
                self.reply(req, &[]);
            }
            (INDEX_ONBOARD, 4) => {
                let c = self.current.to_be_bytes();
                self.reply(req, &c);
            }
            // memoryRead: [sector(2), offset(2)], rest zero
            (INDEX_ONBOARD, 5) if p[4..].iter().all(|&b| b == 0) => {
                let (sector, offset) = (u16at(0), u16at(2) as usize);
                match self.sectors.get(&sector) {
                    Some(s) if offset + 16 <= s.len() => {
                        let chunk = s[offset..offset + 16].to_vec();
                        self.reply(req, &chunk);
                    }
                    _ => self.error(req, 0x02),
                }
            }
            // memoryWriteStart: [sector(2), offset(2), length(2)], rest zero
            (INDEX_ONBOARD, 6) if p[6..].iter().all(|&b| b == 0) => {
                let (sector, offset, len) = (u16at(0), u16at(2), u16at(4));
                self.writes_started.push(sector);
                if (1..0x100).contains(&sector) && offset == 0 && len == self.sector_size {
                    self.write_target = Some(sector);
                    self.write_buf.clear();
                    self.reply(req, &[]);
                } else {
                    self.error(req, 0x02);
                }
            }
            (INDEX_ONBOARD, 7) if self.write_target.is_some() => {
                self.write_buf.extend_from_slice(p);
                self.reply(req, &[]);
            }
            (INDEX_ONBOARD, 8) if self.fail_write_end => {
                self.write_target = None;
                self.error(req, 0x04);
            }
            (INDEX_ONBOARD, 8) => match self.write_target.take() {
                Some(sector) if self.write_buf.len() == self.sector_size as usize => {
                    let mut data = std::mem::take(&mut self.write_buf);
                    if std::mem::take(&mut self.corrupt_next_write) {
                        data[10] ^= 0xFF;
                    }
                    self.sectors.insert(sector, data);
                    self.reply(req, &[]);
                }
                _ => self.error(req, 0x02),
            },
            _ => self.error(req, 0x07),
        }
        Ok(())
    }

    fn read(&mut self, _timeout_ms: i32) -> Result<Option<Report>, Error> {
        Ok(self.outbox.pop_front())
    }
}
