//! Raw 20-byte HID++ "long" reports in and out of the mouse.

use crate::error::Error;

pub const REPORT_LEN: usize = 20;
pub type Report = [u8; REPORT_LEN];

pub const LOGITECH_VID: u16 = 0x046D;
/// Wired G502 HERO (also sold as G502 SE HERO).
pub const G502_HERO_PID: u16 = 0xC08B;

pub trait Transport: Send {
    fn write(&mut self, report: &Report) -> Result<(), Error>;
    /// Waits up to `timeout_ms` for one input report. `Ok(None)` means nothing arrived.
    fn read(&mut self, timeout_ms: i32) -> Result<Option<Report>, Error>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write(&mut self, report: &Report) -> Result<(), Error> {
        (**self).write(report)
    }
    fn read(&mut self, timeout_ms: i32) -> Result<Option<Report>, Error> {
        (**self).read(timeout_ms)
    }
}

/// What to try when the mouse is found but can't be opened.
#[cfg(target_os = "linux")]
const OPEN_HINT: &str = "Your user needs access to /dev/hidraw* (see the README's udev rule).";
#[cfg(not(target_os = "linux"))]
const OPEN_HINT: &str = "Close Logitech G Hub and try again.";

/// Opens the mouse's HID++ long-report interface (vendor usage page `0xFF00`, usage `0x0002`).
pub fn open_g502() -> Result<Box<dyn Transport>, Error> {
    use hidapi::HidApi;

    let api = HidApi::new().map_err(|e| Error::NotFound(format!("USB HID is unavailable: {e}")))?;
    let info = api
        .device_list()
        .find(|d| {
            d.vendor_id() == LOGITECH_VID
                && d.product_id() == G502_HERO_PID
                && d.usage_page() == 0xFF00
                && d.usage() == 0x0002
        })
        .ok_or_else(|| {
            Error::NotFound("Plug in the mouse with its USB cable, then press Refresh.".into())
        })?;
    let device = info
        .open_device(&api)
        .map_err(|e| Error::NotFound(format!("Found the mouse but couldn't open it ({e}). {OPEN_HINT}")))?;
    Ok(Box::new(HidapiTransport { device }))
}

struct HidapiTransport {
    device: hidapi::HidDevice,
}

impl Transport for HidapiTransport {
    fn write(&mut self, report: &Report) -> Result<(), Error> {
        let written = self
            .device
            .write(report)
            .map_err(|e| Error::Connection(format!("write failed: {e}")))?;
        if written < REPORT_LEN {
            return Err(Error::Connection(format!("short write ({written} of {REPORT_LEN} bytes)")));
        }
        Ok(())
    }

    fn read(&mut self, timeout_ms: i32) -> Result<Option<Report>, Error> {
        let mut buf = [0u8; REPORT_LEN];
        let n = self
            .device
            .read_timeout(&mut buf, timeout_ms)
            .map_err(|e| Error::Connection(format!("read failed: {e}")))?;
        Ok((n == REPORT_LEN).then_some(buf))
    }
}
