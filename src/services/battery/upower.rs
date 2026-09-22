//! The doorbell.
//!
//! sysfs has nothing to wait on, so on its own the bar can only poll — and a
//! charger plugged in a second after a poll sits unseen until the next one.
//! UPower already listens to the kernel's uevents and republishes them on
//! D-Bus, so the bar rings off that and re-reads sysfs the moment it does.

use futures_util::{Stream, StreamExt};
use zbus::{proxy, Connection};

const SERVICE: &str = "org.freedesktop.UPower";
/// The aggregate device, which UPower synthesises even where there is one
/// cell — and which follows the same cell the sysfs read picks.
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";

#[proxy(interface = "org.freedesktop.UPower.Device", gen_blocking = false)]
pub trait Device {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
}

pub async fn display_device() -> zbus::Result<DeviceProxy<'static>> {
    let bus = Connection::system().await?;
    let device = DeviceProxy::builder(&bus)
        .destination(SERVICE)?
        .path(DISPLAY_DEVICE)?
        .build()
        .await?;
    // Building a proxy does not talk to the bus, so a read is what actually
    // decides whether the daemon is there.
    device.state().await?;
    Ok(device)
}

/// Rings once per change UPower reports. The caller re-reads sysfs rather than
/// taking the payload: the reading it already knows how to make is the whole
/// truth, and these two properties move together often enough that tracking
/// them apart would only add wakes.
pub async fn rings<'a>(device: &'a DeviceProxy<'_>) -> impl Stream<Item = ()> + 'a {
    let state = device.receive_state_changed().await;
    let level = device.receive_percentage_changed().await;
    futures_util::stream::select(state.map(|_| ()), level.map(|_| ()))
}
