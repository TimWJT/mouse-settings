//! The 256-byte onboard profile, decoded into settings and edited without losing anything.
//!
//! Layout (HID++ 2.0 onboard profile format, as documented by libratbag's
//! `hidpp20_internal_profile` and confirmed against a real G502 HERO dump):
//!
//! | Offset | Size | Field |
//! | ---: | ---: | --- |
//! | 0 | 1 | report interval in ms (1, 2, 4, 8) |
//! | 1 | 1 | default DPI stage index |
//! | 2 | 1 | DPI-shift ("sniper") stage index |
//! | 3 | 10 | five DPI stages, u16 little-endian, 0 = unused |
//! | 13 | 19 | colour, power and reserved bytes (never touched) |
//! | 32 | 64 | 16 button bindings × 4 bytes |
//! | 96 | 64 | 16 G-Shift bindings × 4 bytes |
//! | 160 | 48 | profile name, UTF-16LE |
//! | 208 | 22 | 2 lighting zones × 11 bytes |
//! | 230 | 24 | alternate lighting and free bytes (never touched) |
//! | 254 | 2 | CRC-CCITT of bytes 0..254, big-endian |
//!
//! **Safety rule:** an edit starts from the exact bytes read from the mouse and only rewrites
//! a field whose decoded value actually changed. Anything this app doesn't understand
//! (macros, unknown effects, reserved bytes) is carried through byte for byte.

use serde::{Deserialize, Serialize};

use crate::error::Error;

pub const PROFILE_LEN: usize = 256;
const CRC_AT: usize = 254;

const DPI_AT: usize = 3;
pub const MAX_DPI_STAGES: usize = 5;
pub const DPI_MIN: u16 = 100;
pub const DPI_MAX: u16 = 25600;
pub const DPI_STEP: u16 = 50;

const BUTTONS_AT: usize = 32;
const GSHIFT_AT: usize = 96;
pub const BINDING_SLOTS: usize = 16;
const NAME_AT: usize = 160;
const NAME_LEN: usize = 48;
const LEDS_AT: usize = 208;
pub const LED_LEN: usize = 11;
pub const LED_ZONES: usize = 2;

/// CRC-CCITT (poly 0x1021, init 0xFFFF, no final XOR).
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in bytes {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// What one button does. Anything not recognised is kept as `Raw`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Binding {
    /// Mouse button 1..=16 (1 left, 2 right, 3 middle, 4 back, 5 forward).
    Mouse { button: u8 },
    /// A keyboard key (HID usage id) with a modifier bitmask
    /// (1 Ctrl, 2 Shift, 4 Alt, 8 Win; ×16 for the right-hand keys).
    Key { modifiers: u8, key: u8 },
    /// A consumer-control usage (media keys, volume).
    Media { usage: u16 },
    /// A mouse function such as DPI up/down or profile cycle (see `special_name`).
    Special { action: u8 },
    /// The button does nothing.
    Disabled,
    /// Bytes this app doesn't decode (macros and anything unknown), kept exactly.
    Raw { bytes: [u8; 4] },
}

impl Binding {
    pub fn decode(b: [u8; 4]) -> Self {
        match b {
            [0x80, 0x01, hi, lo] => {
                let mask = u16::from_be_bytes([hi, lo]);
                if mask.count_ones() == 1 {
                    Binding::Mouse { button: mask.trailing_zeros() as u8 + 1 }
                } else {
                    Binding::Raw { bytes: b }
                }
            }
            [0x80, 0x02, modifiers, key] => Binding::Key { modifiers, key },
            [0x80, 0x03, hi, lo] => Binding::Media { usage: u16::from_be_bytes([hi, lo]) },
            [0x90, action, 0x00, 0x00] => Binding::Special { action },
            [0xFF, 0xFF, 0xFF, 0xFF] => Binding::Disabled,
            _ => Binding::Raw { bytes: b },
        }
    }

    pub fn encode(&self) -> Result<[u8; 4], Error> {
        Ok(match *self {
            Binding::Mouse { button } => {
                if !(1..=16).contains(&button) {
                    return Err(Error::Invalid(format!("Mouse button {button} doesn't exist")));
                }
                let [hi, lo] = (1u16 << (button - 1)).to_be_bytes();
                [0x80, 0x01, hi, lo]
            }
            Binding::Key { modifiers, key } => {
                if key == 0 {
                    return Err(Error::Invalid("Pick a key for the keyboard shortcut".into()));
                }
                [0x80, 0x02, modifiers, key]
            }
            Binding::Media { usage } => {
                let [hi, lo] = usage.to_be_bytes();
                [0x80, 0x03, hi, lo]
            }
            Binding::Special { action } => [0x90, action, 0x00, 0x00],
            Binding::Disabled => [0xFF; 4],
            Binding::Raw { bytes } => bytes,
        })
    }
}

/// One lighting zone. Anything not recognised is kept as `Raw`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Lighting {
    Off,
    /// Solid colour, `[r, g, b]`.
    Fixed { color: [u8; 3] },
    /// Pulses one colour. `period_ms` is one full breath; `brightness` is 1..=100.
    Breathing { color: [u8; 3], period_ms: u16, brightness: u8 },
    /// Cycles through all colours.
    Cycle { period_ms: u16, brightness: u8 },
    Raw { bytes: [u8; LED_LEN] },
}

impl Lighting {
    pub fn decode(b: [u8; LED_LEN]) -> Self {
        let rest_zero = |from: usize| b[from..].iter().all(|&x| x == 0);
        // Brightness byte 0 means "full" on these devices.
        let level = |x: u8| if x == 0 { 100 } else { x.min(100) };
        match b[0] {
            0x00 if rest_zero(1) => Lighting::Off,
            0x01 if rest_zero(4) => Lighting::Fixed { color: [b[1], b[2], b[3]] },
            // Byte 6 is the waveform; only the plain one (0) is decoded, others stay Raw
            // so editing the colour can't silently change the waveform.
            0x0A if b[6] == 0 && rest_zero(8) => Lighting::Breathing {
                color: [b[1], b[2], b[3]],
                period_ms: u16::from_be_bytes([b[4], b[5]]),
                brightness: level(b[7]),
            },
            0x03 if b[1..6].iter().all(|&x| x == 0) && rest_zero(9) => Lighting::Cycle {
                period_ms: u16::from_be_bytes([b[6], b[7]]),
                brightness: level(b[8]),
            },
            _ => Lighting::Raw { bytes: b },
        }
    }

    pub fn encode(&self) -> Result<[u8; LED_LEN], Error> {
        let mut b = [0u8; LED_LEN];
        let check = |period: u16, brightness: u8| {
            if !(1000..=20000).contains(&period) {
                return Err(Error::Invalid("Effect speed must be 1 to 20 seconds".into()));
            }
            if !(1..=100).contains(&brightness) {
                return Err(Error::Invalid("Brightness must be 1 to 100%".into()));
            }
            Ok(())
        };
        match *self {
            Lighting::Off => {}
            Lighting::Fixed { color } => {
                b[0] = 0x01;
                b[1..4].copy_from_slice(&color);
            }
            Lighting::Breathing { color, period_ms, brightness } => {
                check(period_ms, brightness)?;
                b[0] = 0x0A;
                b[1..4].copy_from_slice(&color);
                b[4..6].copy_from_slice(&period_ms.to_be_bytes());
                b[7] = brightness;
            }
            Lighting::Cycle { period_ms, brightness } => {
                check(period_ms, brightness)?;
                b[0] = 0x03;
                b[6..8].copy_from_slice(&period_ms.to_be_bytes());
                b[8] = brightness;
            }
            Lighting::Raw { bytes } => b = bytes,
        }
        Ok(b)
    }
}

/// Everything the page can show and edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// `None` when the byte isn't a known interval; it's then left alone.
    pub report_rate_hz: Option<u16>,
    pub dpi_stages: Vec<u16>,
    pub default_stage: u8,
    pub shift_stage: u8,
    pub buttons: Vec<Binding>,
    pub gshift_buttons: Vec<Binding>,
    pub lighting: Vec<Lighting>,
    /// Shown only; this app doesn't rename profiles.
    pub name: String,
}

fn rate_from_interval(ms: u8) -> Option<u16> {
    match ms {
        1 => Some(1000),
        2 => Some(500),
        4 => Some(250),
        8 => Some(125),
        _ => None,
    }
}

fn interval_from_rate(hz: u16) -> Result<u8, Error> {
    match hz {
        1000 => Ok(1),
        500 => Ok(2),
        250 => Ok(4),
        125 => Ok(8),
        _ => Err(Error::Invalid(format!("{hz} Hz isn't a supported polling rate"))),
    }
}

/// A profile exactly as stored on the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileImage {
    bytes: [u8; PROFILE_LEN],
}

impl ProfileImage {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; PROFILE_LEN] = bytes
            .try_into()
            .map_err(|_| Error::Invalid(format!("A profile is {PROFILE_LEN} bytes, got {}", bytes.len())))?;
        Ok(ProfileImage { bytes })
    }

    pub fn bytes(&self) -> &[u8; PROFILE_LEN] {
        &self.bytes
    }

    pub fn stored_crc(&self) -> u16 {
        u16::from_be_bytes([self.bytes[CRC_AT], self.bytes[CRC_AT + 1]])
    }

    pub fn crc_ok(&self) -> bool {
        crc16(&self.bytes[..CRC_AT]) == self.stored_crc()
    }

    fn binding(&self, at: usize, slot: usize) -> Binding {
        let o = at + slot * 4;
        Binding::decode(self.bytes[o..o + 4].try_into().unwrap())
    }

    fn lighting_at(&self, zone: usize) -> Lighting {
        let o = LEDS_AT + zone * LED_LEN;
        Lighting::decode(self.bytes[o..o + LED_LEN].try_into().unwrap())
    }

    fn name(&self) -> String {
        let units: Vec<u16> = self.bytes[NAME_AT..NAME_AT + NAME_LEN]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0 && u != 0xFFFF)
            .collect();
        String::from_utf16_lossy(&units)
    }

    pub fn settings(&self) -> Settings {
        let mut dpi_stages = Vec::new();
        for i in 0..MAX_DPI_STAGES {
            let o = DPI_AT + i * 2;
            let v = u16::from_le_bytes([self.bytes[o], self.bytes[o + 1]]);
            if v == 0 || v == 0xFFFF {
                break;
            }
            dpi_stages.push(v);
        }
        Settings {
            report_rate_hz: rate_from_interval(self.bytes[0]),
            dpi_stages,
            default_stage: self.bytes[1],
            shift_stage: self.bytes[2],
            buttons: (0..BINDING_SLOTS).map(|i| self.binding(BUTTONS_AT, i)).collect(),
            gshift_buttons: (0..BINDING_SLOTS).map(|i| self.binding(GSHIFT_AT, i)).collect(),
            lighting: (0..LED_ZONES).map(|z| self.lighting_at(z)).collect(),
            name: self.name(),
        }
    }

    /// Returns a copy with `wanted` applied. Only fields whose decoded value differs from
    /// this image are re-encoded; every other byte stays exactly as it was. The CRC is
    /// recomputed only if something changed.
    pub fn with_settings(&self, wanted: &Settings) -> Result<ProfileImage, Error> {
        if !self.crc_ok() {
            return Err(Error::Refused(
                "the profile on the mouse fails its own checksum, so it isn't safe to edit".into(),
            ));
        }
        let now = self.settings();
        let mut out = *self;
        let b = &mut out.bytes;

        if wanted.report_rate_hz != now.report_rate_hz {
            let hz = wanted
                .report_rate_hz
                .ok_or_else(|| Error::Invalid("Pick a polling rate".into()))?;
            b[0] = interval_from_rate(hz)?;
        }

        if wanted.dpi_stages != now.dpi_stages {
            let n = wanted.dpi_stages.len();
            if !(1..=MAX_DPI_STAGES).contains(&n) {
                return Err(Error::Invalid(format!("Use 1 to {MAX_DPI_STAGES} DPI stages")));
            }
            for (i, slot) in b[DPI_AT..DPI_AT + MAX_DPI_STAGES * 2].chunks_exact_mut(2).enumerate() {
                let v = match wanted.dpi_stages.get(i) {
                    Some(&dpi) => {
                        if !(DPI_MIN..=DPI_MAX).contains(&dpi) || dpi % DPI_STEP != 0 {
                            return Err(Error::Invalid(format!(
                                "{dpi} DPI isn't allowed: use {DPI_MIN} to {DPI_MAX} in steps of {DPI_STEP}"
                            )));
                        }
                        dpi
                    }
                    None => 0,
                };
                slot.copy_from_slice(&v.to_le_bytes());
            }
        }
        let stages = wanted.dpi_stages.len() as u8;
        for (label, wanted_idx, now_idx, at) in [
            ("starting", wanted.default_stage, now.default_stage, 1),
            ("DPI-shift", wanted.shift_stage, now.shift_stage, 2),
        ] {
            if wanted_idx != now_idx || wanted.dpi_stages != now.dpi_stages {
                if wanted_idx >= stages {
                    return Err(Error::Invalid(format!("The {label} DPI stage doesn't exist")));
                }
                b[at] = wanted_idx;
            }
        }

        for (at, wanted_list, now_list, label) in [
            (BUTTONS_AT, &wanted.buttons, &now.buttons, "buttons"),
            (GSHIFT_AT, &wanted.gshift_buttons, &now.gshift_buttons, "G-Shift buttons"),
        ] {
            if wanted_list.len() != BINDING_SLOTS {
                return Err(Error::Invalid(format!("Expected {BINDING_SLOTS} {label}")));
            }
            for (i, (w, n)) in wanted_list.iter().zip(now_list.iter()).enumerate() {
                if w != n {
                    let o = at + i * 4;
                    b[o..o + 4].copy_from_slice(&w.encode()?);
                }
            }
        }

        if wanted.lighting.len() != LED_ZONES {
            return Err(Error::Invalid(format!("Expected {LED_ZONES} lighting zones")));
        }
        for (z, (w, n)) in wanted.lighting.iter().zip(now.lighting.iter()).enumerate() {
            if w != n {
                let o = LEDS_AT + z * LED_LEN;
                b[o..o + LED_LEN].copy_from_slice(&w.encode()?);
            }
        }

        if out.bytes[..CRC_AT] != self.bytes[..CRC_AT] {
            let crc = crc16(&out.bytes[..CRC_AT]);
            out.bytes[CRC_AT..].copy_from_slice(&crc.to_be_bytes());
        }
        Ok(out)
    }

    /// Byte offsets that differ from `other`.
    pub fn diff(&self, other: &ProfileImage) -> Vec<usize> {
        (0..PROFILE_LEN).filter(|&i| self.bytes[i] != other.bytes[i]).collect()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A real G502 HERO onboard profile (sector 1), CRC 0xE2CC.
    pub const SAMPLE_HEX: &str = "\
        02 02 02 2C 01 26 02 84 03 00 00 00 00 FF FF FF \
        FF 00 FF FF FF FF FF FF FF FF FF FF FF FF FF FF \
        80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08 \
        80 01 00 10 80 02 00 30 80 02 04 2B 90 00 00 00 \
        90 0A 00 00 90 02 00 00 90 01 00 00 FF FF FF FF \
        FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF \
        80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08 \
        80 01 00 10 80 02 00 30 80 02 04 2B 90 00 00 00 \
        90 0A 00 00 90 02 00 00 90 01 00 00 FF FF FF FF \
        FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF FF \
        50 00 52 00 4F 00 46 00 49 00 4C 00 45 00 5F 00 \
        4E 00 41 00 4D 00 45 00 5F 00 44 00 45 00 46 00 \
        41 00 55 00 4C 00 54 00 00 00 00 00 00 00 00 00 \
        01 FF 00 04 00 00 00 00 00 00 00 01 FF 00 04 00 \
        00 00 00 00 00 00 FF FF FF FF FF FF FF FF FF FF \
        FF FF FF FF FF FF FF FF FF FF FF FF 00 FF E2 CC";

    pub fn sample() -> ProfileImage {
        let bytes: Vec<u8> = SAMPLE_HEX
            .split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect();
        ProfileImage::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn sample_checksum_matches() {
        let p = sample();
        assert!(p.crc_ok());
        assert_eq!(p.stored_crc(), 0xE2CC);
    }

    #[test]
    fn sample_decodes_to_expected_settings() {
        let s = sample().settings();
        assert_eq!(s.report_rate_hz, Some(500));
        assert_eq!(s.dpi_stages, vec![300, 550, 900]);
        assert_eq!((s.default_stage, s.shift_stage), (2, 2));
        assert_eq!(s.buttons[0], Binding::Mouse { button: 1 });
        assert_eq!(s.buttons[4], Binding::Mouse { button: 5 });
        assert_eq!(s.buttons[6], Binding::Key { modifiers: 0x04, key: 0x2B }); // Alt+Tab
        assert_eq!(s.buttons[8], Binding::Special { action: 0x0A });
        assert_eq!(s.buttons[11], Binding::Disabled);
        assert_eq!(s.lighting[0], Lighting::Fixed { color: [0xFF, 0x00, 0x04] });
        assert_eq!(s.name, "PROFILE_NAME_DEFAULT");
    }

    #[test]
    fn unchanged_settings_give_identical_bytes() {
        let p = sample();
        let same = p.with_settings(&p.settings()).unwrap();
        assert_eq!(same, p);
    }

    #[test]
    fn editing_one_field_touches_only_that_field_and_the_crc() {
        let p = sample();
        let mut s = p.settings();
        s.dpi_stages[0] = 400;
        let edited = p.with_settings(&s).unwrap();
        // 300 = 2C 01 and 400 = 90 01 (little-endian), so only byte 3 changes.
        assert_eq!(edited.diff(&p), vec![3, 254, 255]);
        assert!(edited.crc_ok());
        assert_eq!(edited.settings().dpi_stages, vec![400, 550, 900]);
    }

    #[test]
    fn every_binding_kind_round_trips() {
        for b in [
            Binding::Mouse { button: 3 },
            Binding::Key { modifiers: 0x01, key: 0x06 },
            Binding::Media { usage: 0x00E9 },
            Binding::Special { action: 0x03 },
            Binding::Disabled,
            Binding::Raw { bytes: [0x00, 0x01, 0x02, 0x03] },
        ] {
            assert_eq!(Binding::decode(b.encode().unwrap()), b);
        }
    }

    #[test]
    fn every_lighting_kind_round_trips() {
        for l in [
            Lighting::Off,
            Lighting::Fixed { color: [1, 2, 3] },
            Lighting::Breathing { color: [9, 8, 7], period_ms: 4000, brightness: 60 },
            Lighting::Cycle { period_ms: 10000, brightness: 100 },
            Lighting::Raw { bytes: [0x0B, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10] },
            Lighting::Raw { bytes: [0x0A, 1, 2, 3, 0x0F, 0xA0, 0x01, 50, 0, 0, 0] }, // non-zero waveform
        ] {
            assert_eq!(Lighting::decode(l.encode().unwrap()), l);
        }
    }

    #[test]
    fn json_field_names_match_the_page() {
        let l = Lighting::Breathing { color: [1, 2, 3], period_ms: 4000, brightness: 50 };
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(json, r#"{"kind":"breathing","color":[1,2,3],"periodMs":4000,"brightness":50}"#);
        let b: Binding = serde_json::from_str(r#"{"kind":"key","modifiers":4,"key":43}"#).unwrap();
        assert_eq!(b, Binding::Key { modifiers: 4, key: 43 });
    }

    #[test]
    fn raw_bindings_and_lighting_survive_unrelated_edits() {
        let mut bytes = *sample().bytes();
        bytes[BUTTONS_AT + 5 * 4..BUTTONS_AT + 6 * 4].copy_from_slice(&[0x00, 0x01, 0x00, 0x40]); // a macro
        let crc = crc16(&bytes[..CRC_AT]);
        bytes[CRC_AT..].copy_from_slice(&crc.to_be_bytes());
        let p = ProfileImage::from_bytes(&bytes).unwrap();
        let mut s = p.settings();
        assert!(matches!(s.buttons[5], Binding::Raw { .. }));
        s.report_rate_hz = Some(1000);
        let edited = p.with_settings(&s).unwrap();
        assert_eq!(edited.diff(&p), vec![0, 254, 255]);
    }

    #[test]
    fn invalid_edits_are_rejected() {
        let p = sample();
        let mut s = p.settings();
        s.dpi_stages = vec![125];
        assert!(p.with_settings(&s).is_err(), "not a multiple of 50");
        let mut s = p.settings();
        s.dpi_stages = vec![800];
        s.default_stage = 0;
        s.shift_stage = 1;
        assert!(p.with_settings(&s).is_err(), "shift stage out of range");
        let mut s = p.settings();
        s.report_rate_hz = Some(333);
        assert!(p.with_settings(&s).is_err());
    }

    #[test]
    fn a_profile_with_a_bad_checksum_is_not_edited() {
        let mut bytes = *sample().bytes();
        bytes[0] = 0x01;
        let p = ProfileImage::from_bytes(&bytes).unwrap();
        assert!(!p.crc_ok());
        assert!(matches!(p.with_settings(&p.settings()), Err(Error::Refused(_))));
    }

    #[test]
    fn page_preview_fixture_matches_the_sample() {
        // `src/sample-profile.json` feeds the browser preview; keep it honest.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/sample-profile.json");
        let json = serde_json::to_string_pretty(&sample().settings()).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURE").is_some() {
            std::fs::write(path, &json).unwrap();
        }
        let on_disk = std::fs::read_to_string(path).expect("run once with UPDATE_FIXTURE=1");
        assert_eq!(on_disk.replace("\r\n", "\n"), json);
    }
}
