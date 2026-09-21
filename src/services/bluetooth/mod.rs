//! Bluetooth, over BlueZ.
//!
//! Two switches, not one. rfkill is the kernel's, answers with no daemon at
//! all, and is the only thing that can report a hardware block; BlueZ's
//! `Powered` is the user-facing one and the only one a session user may write
//! without root. The panel's toggle writes `Powered` and is drawn disabled
//! when rfkill says the radio is held down, so it never fails silently.

mod device;
mod session;

use std::sync::Arc;

pub use bluer::Address;
pub use device::{BtDevice, DeviceKind};

use crate::services::{
    bus::Backend,
    rfkill::{self, Block},
    status::{Availability, Failure, Interest},
};

/// How many nearby strangers the panel will list. A busy café produces dozens
/// of beacons; a bar is not a scanner.
pub const MAX_DISCOVERED: usize = 6;

/// Why the radio is up or down, which decides whether the toggle is live.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadioState {
    #[default]
    Unknown,
    /// A physical switch is holding it down. Drawn, but not pressable.
    HardBlocked,
    Off,
    On,
}

impl RadioState {
    pub fn on(self) -> bool {
        matches!(self, Self::On)
    }

    /// A hard block cannot be cleared from here, so the switch is inert.
    pub fn changeable(self) -> bool {
        !matches!(self, Self::HardBlocked)
    }

    /// A hard block outranks anything BlueZ believes: `Powered` can still read
    /// true against a radio the firmware has already cut.
    fn resolve(block: Block, powered: Option<bool>) -> Self {
        match (block, powered) {
            (Block::Hard, _) => Self::HardBlocked,
            (Block::Absent, None) => Self::Unknown,
            (_, Some(true)) => Self::On,
            (_, Some(false)) => Self::Off,
            (Block::Unblocked, None) => Self::On,
            _ => Self::Off,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BluetoothState {
    pub availability: Availability,
    pub radio: RadioState,
    /// The adapter's friendly name, for the panel subtitle.
    pub adapter: Option<String>,
    /// Every paired device, in range or not. Connected first, then by name.
    pub paired: Vec<BtDevice>,
    /// Unpaired devices seen since discovery started, strongest first.
    pub discovered: Vec<BtDevice>,
    pub discovering: bool,
    pub failure: Option<Failure>,
}

impl BluetoothState {
    pub fn connected(&self) -> impl Iterator<Item = &BtDevice> {
        self.paired.iter().filter(|device| device.connected)
    }

    /// What the pill says beside the glyph: the one connected device, or a
    /// count once there is more than one.
    pub fn summary(&self) -> Option<String> {
        let mut connected = self.connected();
        let first = connected.next()?;
        match connected.count() {
            0 => Some(first.name.clone()),
            rest => Some(format!("{} Connected", rest + 2)),
        }
    }

    pub fn device(&self, address: Address) -> Option<&BtDevice> {
        self.paired
            .iter()
            .chain(self.discovered.iter())
            .find(|device| device.address == address)
    }

    /// Connected first, then alphabetical; nearby strangers by signal.
    fn sort(&mut self) {
        self.paired.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.name.cmp(&b.name))
        });
        self.discovered.sort_by(|a, b| {
            b.rssi
                .unwrap_or(i16::MIN)
                .cmp(&a.rssi.unwrap_or(i16::MIN))
                .then_with(|| a.name.cmp(&b.name))
        });
        self.discovered.truncate(MAX_DISCOVERED);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BluetoothCommand {
    /// Writes BlueZ `Powered`. Refused while the radio is hard-blocked.
    SetPowered(bool),
    Connect(Address),
    Disconnect(Address),
    /// Pair, trust, then connect — the three calls a user means by "connect to
    /// this new thing".
    PairAndConnect(Address),
    /// `adapter.remove_device`.
    Forget(Address),
    /// Discovery runs only while the panel is up: a scan left going is a
    /// battery drain and a privacy leak.
    Interest(Interest),
}

pub type Channel = crate::services::bus::Channel<BluetoothState, BluetoothCommand>;

pub async fn run(backend: Backend<BluetoothState, BluetoothCommand>) {
    let Backend {
        publish,
        commands,
    } = backend;

    // rfkill needs no daemon, so the icon is right even where the rest of this
    // service never starts.
    let block = rfkill::block(rfkill::BLUETOOTH);
    publish.edit(|state| {
        state.radio = RadioState::resolve(block, None);
        state.availability = match block {
            Block::Absent => Availability::Unavailable(Arc::from("no bluetooth radio")),
            _ => Availability::Starting,
        };
        true
    });
    if !block.present() {
        return;
    }

    session::run(publish, commands).await;
}
