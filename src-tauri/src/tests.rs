//! End-to-end tests against the simulated mouse.

use crate::device::{Mode, Mouse, Slot};
use crate::error::Error;
use crate::hidpp::{build_request, Hidpp, Transport};
use crate::mock::MockMouse;
use crate::profile::{tests::sample, Binding, Lighting, ProfileImage};

fn mouse() -> Mouse<MockMouse> {
    Mouse::new(MockMouse::new())
}

fn no_backup(_: &ProfileImage) -> Result<String, Error> {
    Ok("backup.bin".into())
}

fn sim(m: &mut Mouse<MockMouse>) -> &mut MockMouse {
    m.hidpp.transport_mut()
}

#[test]
fn request_frame_layout() {
    let r = build_request(0x03, 0x05, &[0x00, 0x01, 0x00, 0xF0]);
    assert_eq!(&r[..9], &[0x11, 0xFF, 0x03, 0x5E, 0x00, 0x01, 0x00, 0xF0, 0x00]);
}

#[test]
fn info_reads_everything() {
    let mut m = mouse();
    let info = m.info().unwrap();
    assert_eq!(info.name, "G502 HERO");
    assert_eq!(info.firmware.as_deref(), Some("U1 27.03.B0016"));
    assert_eq!(info.protocol, "4.2");
    assert_eq!(info.mode, Mode::Onboard);
    assert_eq!(info.description.button_count, 11);
    assert_eq!(info.current_profile, 1);
    assert_eq!(
        info.slots,
        vec![
            Slot { address: 1, enabled: true, writable: true },
            Slot { address: 2, enabled: false, writable: true },
            Slot { address: 0x0101, enabled: true, writable: false },
        ]
    );
}

#[test]
fn feature_indexes_are_cached() {
    let mut h = Hidpp::new(MockMouse::new());
    assert_eq!(h.feature_index(0x8100).unwrap(), 3);
    h.transport_mut().unplugged = true;
    assert_eq!(h.feature_index(0x8100).unwrap(), 3, "served from cache, no I/O");
    assert!(matches!(h.feature_index(0x1234), Err(Error::Connection(_))));
}

#[test]
fn unsupported_feature_is_reported() {
    let mut h = Hidpp::new(MockMouse::new());
    assert_eq!(h.feature_index(0x1234), Err(Error::Unsupported(0x1234)));
}

#[test]
fn device_errors_are_decoded() {
    let mut m = mouse();
    // Sector 9 doesn't exist on the simulated mouse.
    let err = m.read_sector(9, 256).unwrap_err();
    assert!(matches!(err, Error::Device { feature: 0x8100, function: 5, code: 0x02 }), "{err:?}");
}

#[test]
fn read_profile_returns_the_stored_bytes() {
    let mut m = mouse();
    assert_eq!(m.read_profile(1).unwrap(), sample());
}

#[test]
fn partial_last_chunk_is_read_at_the_sector_end() {
    let mut m = mouse();
    let bytes = m.read_sector(1, 250).unwrap();
    assert_eq!(bytes, sample().bytes()[..250].to_vec());
}

#[test]
fn save_writes_only_the_target_sector_and_reloads_it() {
    let mut m = mouse();
    let current = m.read_profile(1).unwrap();
    let mut s = current.settings();
    s.dpi_stages = vec![800, 1600];
    s.default_stage = 0;
    s.shift_stage = 0;
    s.buttons[5] = Binding::Media { usage: 0x00CD };
    s.lighting[1] = Lighting::Off;
    let wanted = current.with_settings(&s).unwrap();

    let mut backed_up = None;
    let report = m
        .write_profile(1, &wanted, |old| {
            backed_up = Some(*old);
            Ok("profile-1-1.bin".into())
        })
        .unwrap();
    assert_eq!(backed_up, Some(sample()), "old profile is backed up before writing");
    assert_eq!(report.backup_file.as_deref(), Some("profile-1-1.bin"));
    assert!(report.reloaded);
    assert_eq!(sim(&mut m).writes_started, vec![1]);
    assert_eq!(sim(&mut m).reloads, 1);
    assert_eq!(m.read_profile(1).unwrap().settings(), s);
}

#[test]
fn saving_identical_settings_writes_nothing() {
    let mut m = mouse();
    let report = m.write_profile(1, &sample(), no_backup).unwrap();
    assert_eq!(report.bytes_changed, 0);
    assert!(sim(&mut m).writes_started.is_empty());
}

fn edited() -> ProfileImage {
    let mut s = sample().settings();
    s.report_rate_hz = Some(1000);
    sample().with_settings(&s).unwrap()
}

#[test]
fn refuses_disabled_missing_and_factory_profiles() {
    for (address, needle) in [(2, "switched off"), (3, "isn't set up"), (0x0101, "factory")] {
        let mut m = mouse();
        let err = m.write_profile(address, &edited(), no_backup).unwrap_err().to_string();
        assert!(err.contains(needle), "{address}: {err}");
        assert!(sim(&mut m).writes_started.is_empty());
    }
}

#[test]
fn refuses_unexpected_sector_size() {
    let mut m = mouse();
    sim(&mut m).sector_size = 255;
    let err = m.write_profile(1, &edited(), no_backup).unwrap_err().to_string();
    assert!(err.contains("255-byte"), "{err}");
    assert!(sim(&mut m).writes_started.is_empty());
}

#[test]
fn failed_backup_stops_the_write() {
    let mut m = mouse();
    let err = m
        .write_profile(1, &edited(), |_| Err(Error::Refused("disk full".into())))
        .unwrap_err();
    assert!(err.to_string().contains("disk full"));
    assert!(sim(&mut m).writes_started.is_empty());
}

#[test]
fn readback_mismatch_is_reported_with_the_backup_name() {
    let mut m = mouse();
    sim(&mut m).corrupt_next_write = true;
    let err = m.write_profile(1, &edited(), |_| Ok("profile-1-9.bin".into())).unwrap_err();
    assert!(err.to_string().contains("profile-1-9.bin"), "{err}");
}

#[test]
fn non_current_profile_is_written_without_switching_to_it() {
    let mut m = mouse();
    sim(&mut m).current = 0x0101;
    let report = m.write_profile(1, &edited(), no_backup).unwrap();
    assert!(!report.reloaded);
    assert_eq!(sim(&mut m).current, 0x0101);
}

#[test]
fn mode_switch() {
    let mut m = mouse();
    m.set_mode(false).unwrap();
    assert_eq!(m.mode().unwrap(), Mode::Host);
    m.set_mode(true).unwrap();
    assert_eq!(m.mode().unwrap(), Mode::Onboard);
}

#[test]
fn transport_errors_drop_the_connection_device_errors_do_not() {
    assert!(Error::Connection("x".into()).drops_connection());
    assert!(Error::Timeout { feature: 0, function: 0 }.drops_connection());
    assert!(!Error::Device { feature: 0, function: 0, code: 2 }.drops_connection());
    assert!(!Error::Refused("x".into()).drops_connection());
}

#[test]
fn timeout_when_the_mouse_never_answers() {
    struct Silent;
    impl Transport for Silent {
        fn write(&mut self, _: &crate::hidpp::Report) -> Result<(), Error> {
            Ok(())
        }
        fn read(&mut self, timeout_ms: i32) -> Result<Option<crate::hidpp::Report>, Error> {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms.min(50) as u64));
            Ok(None)
        }
    }
    let mut h = Hidpp::new(Silent);
    assert!(matches!(h.feature_index(0x8100), Err(Error::Timeout { .. })));
}

#[test]
fn refuses_a_profile_list_with_a_bad_checksum() {
    let mut m = mouse();
    sim(&mut m).sectors.get_mut(&0).unwrap()[3] ^= 0x01;
    let err = m.write_profile(1, &edited(), no_backup).unwrap_err().to_string();
    assert!(err.contains("profile list"), "{err}");
    assert!(sim(&mut m).writes_started.is_empty());
}

#[test]
fn save_settings_refuses_when_the_mouse_changed_since_loading() {
    let mut m = mouse();
    let base = m.read_profile(1).unwrap().settings();
    let mut wanted = base.clone();
    wanted.dpi_stages[0] = 400;
    // Something else changes the polling rate on the mouse after the page loaded.
    let mut stale = base.clone();
    stale.report_rate_hz = Some(1000);
    let changed = sample().with_settings(&stale).unwrap();
    sim(&mut m).sectors.insert(1, changed.bytes().to_vec());
    let err = m.save_settings(1, &base, &wanted, no_backup).unwrap_err().to_string();
    assert!(err.contains("changed since it was loaded"), "{err}");
    assert!(sim(&mut m).writes_started.is_empty());
}

#[test]
fn save_settings_applies_only_the_edits() {
    let mut m = mouse();
    let base = m.read_profile(1).unwrap().settings();
    let mut wanted = base.clone();
    wanted.dpi_stages[0] = 400;
    let report = m.save_settings(1, &base, &wanted, no_backup).unwrap();
    assert_eq!(report.bytes_changed, 3, "one DPI byte and the two CRC bytes");
    assert_eq!(m.read_profile(1).unwrap().settings(), wanted);
}

#[test]
fn a_failure_mid_write_names_the_backup() {
    let mut m = mouse();
    sim(&mut m).fail_write_end = true;
    let err = m.write_profile(1, &edited(), |_| Ok("profile-1-7.bin".into())).unwrap_err();
    assert!(matches!(err, Error::AfterWrite { .. }), "{err:?}");
    assert!(err.to_string().contains("profile-1-7.bin"), "{err}");
}
