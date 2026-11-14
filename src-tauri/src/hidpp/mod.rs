//! HID++ 2.0 request/response framing with a per-connection feature-index cache.
//!
//! Frame layout (long report): `[0x11, device, feature_index, function << 4 | sw_id, params × 16]`.
//! Error frame: `[0x11, device, 0xFF, feature_index, function << 4 | sw_id, error_code, ...]`.

pub mod transport;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::Error;
pub use transport::{Report, Transport, REPORT_LEN};

pub const REPORT_ID_LONG: u8 = 0x11;
/// Device index for a mouse plugged in by cable (no receiver in between).
pub const DEVICE_WIRED: u8 = 0xFF;
/// Our software id (1..=15). Replies echo it, so we never mistake another app's reply for ours.
pub const SW_ID: u8 = 0x0E;
pub const PARAMS_LEN: usize = REPORT_LEN - 4;
pub type Params = [u8; PARAMS_LEN];

const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);

pub const FEATURE_ROOT: u16 = 0x0000;

pub fn build_request(index: u8, function: u8, params: &[u8]) -> Report {
    let mut r = [0u8; REPORT_LEN];
    r[0] = REPORT_ID_LONG;
    r[1] = DEVICE_WIRED;
    r[2] = index;
    r[3] = (function << 4) | SW_ID;
    let n = params.len().min(PARAMS_LEN);
    r[4..4 + n].copy_from_slice(&params[..n]);
    r
}

pub struct Hidpp<T: Transport> {
    transport: T,
    features: HashMap<u16, u8>,
}

impl<T: Transport> Hidpp<T> {
    pub fn new(transport: T) -> Self {
        Hidpp { transport, features: HashMap::from([(FEATURE_ROOT, 0)]) }
    }

    /// Throws away input reports that arrived since the last command
    /// (button notifications, late replies to a request that timed out).
    pub fn drain(&mut self) -> Result<(), Error> {
        for _ in 0..64 {
            if self.transport.read(0)?.is_none() {
                break;
            }
        }
        Ok(())
    }

    /// Sends one request to a feature *index* and returns the 16 reply parameter bytes.
    fn request(&mut self, feature: u16, index: u8, function: u8, params: &[u8]) -> Result<Params, Error> {
        let req = build_request(index, function, params);
        self.transport.write(&req)?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout { feature, function });
            }
            let Some(r) = self.transport.read(left.as_millis().max(1) as i32)? else { continue };
            if r[0] != REPORT_ID_LONG || r[1] != DEVICE_WIRED {
                continue;
            }
            if r[2] == index && r[3] == req[3] {
                let mut out = [0u8; PARAMS_LEN];
                out.copy_from_slice(&r[4..]);
                return Ok(out);
            }
            if r[2] == 0xFF && r[3] == index && r[4] == req[3] {
                return Err(Error::Device { feature, function, code: r[5] });
            }
            // Anything else is a notification or someone else's reply: keep waiting.
        }
    }

    /// Looks up (and caches) the runtime index of a feature via IRoot.getFeature.
    pub fn feature_index(&mut self, feature: u16) -> Result<u8, Error> {
        if let Some(&i) = self.features.get(&feature) {
            return Ok(i);
        }
        let reply = self.request(FEATURE_ROOT, 0, 0, &feature.to_be_bytes())?;
        if reply[0] == 0 {
            return Err(Error::Unsupported(feature));
        }
        self.features.insert(feature, reply[0]);
        Ok(reply[0])
    }

    /// Calls `function` of `feature` (by feature id, not index).
    pub fn call(&mut self, feature: u16, function: u8, params: &[u8]) -> Result<Params, Error> {
        let index = self.feature_index(feature)?;
        self.request(feature, index, function, params)
    }

    #[cfg(test)]
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}
