//! Static panel glyphs. Unlike the bar's icons these carry no animated state:
//! they are one authored glyph each, stroked at whatever size the caller asks
//! for.
//!
//! The set is Untitled UI's. Every rune is authored in the same 20 × 20 box
//! and fitted from *that* box rather than from its own ink bounds, so a
//! chevron stays a chevron next to a headphone instead of being blown up to
//! fill the same square.

use crate::scene::Scene;
use crownui::prelude::Color;
use kurbo::Rect;

use crownbar_widgets::Rune;

use super::{art::Art, ethernet, untitled::*};

/// How much of the target bounds the view box covers. The caller's box is the
/// icon's whole frame: Untitled carries its own padding.
const INK_FILL: f64 = 1.0;

/// The design file has no gamepad, so this one is drawn to Untitled's rules:
/// the 24 grid scaled into the 20 box, the set's stroke and corner rounding,
/// buttons as the set's 0.0083-long dots.
static GAMEPAD: Art = Art::new(&[
    "M5 9.1667H8.3333M6.6667 7.5V10.8333M12.5 10H12.5083M15 8.3333H15.0083M8.7071 4.1667H11.2929C13.481 4.1667 14.5751 4.1667 15.4312 4.5815C16.1848 4.9466 16.8113 5.5292 17.229 6.2533C17.7036 7.0757 17.7816 8.167 17.9374 10.3496L18.1423 13.2169C18.2431 14.628 17.1255 15.8333 15.7108 15.8333C14.9872 15.8333 14.301 15.5118 13.8377 14.956L13.5294 14.5833C13.2474 14.2448 13.1064 14.0756 12.9457 13.9388C12.6088 13.6523 12.2009 13.4622 11.7652 13.389C11.5573 13.3542 11.3369 13.3542 10.8963 13.3542H9.1037C8.6631 13.3542 8.4427 13.3542 8.2348 13.389C7.7991 13.4622 7.3912 13.6523 7.0543 13.9388C6.8936 14.0756 6.7526 14.2448 6.4706 14.5833L6.1623 14.956C5.699 15.5118 5.0129 15.8333 4.2892 15.8333C2.8745 15.8333 1.7569 14.628 1.8577 13.2169L2.0625 10.3496C2.2184 8.167 2.2963 7.0757 2.771 6.2533C3.1887 5.5292 3.8152 4.9466 4.5688 4.5815C5.4249 4.1667 6.519 4.1667 8.7071 4.1667Z",
]);

fn art(rune: Rune) -> &'static Art {
    match rune {
        Rune::Headphones => &HEADPHONES_02,
        Rune::Speaker => &VOLUME_MAX,
        Rune::Laptop => &LAPTOP_01,
        Rune::Display => &MONITOR_01,
        Rune::Keyboard => &KEYBOARD_01,
        Rune::Gamepad => &GAMEPAD,
        Rune::Phone => &PHONE_01,
        Rune::Microphone => &MICROPHONE_02,
        Rune::Sun => &SUN,
        Rune::Moon => &MOON_01,
        Rune::Wifi => &WIFI,
        Rune::Ethernet => &ethernet::PORT,
        Rune::Bluetooth => &BLUETOOTH_ON,
        Rune::Warning => &ALERT_TRIANGLE,
        Rune::GaugeLow => &GAUGE_LOW,
        Rune::GaugeMid => &GAUGE_MID,
        Rune::GaugeHigh => &GAUGE_HIGH,
        Rune::Window => &BROWSER,
        Rune::Workspace => &GRID_01,
        Rune::Sparkles => &MAGIC_WAND_02,
        Rune::Stopwatch => &CLOCK_STOPWATCH,
        Rune::Clock => &HOURGLASS_01,
        Rune::MusicalNote => &MUSIC_NOTE_01,
        Rune::CommandLine => &TERMINAL_SQUARE,
        Rune::ArrowDownTray => &ARROW_SQUARE_DOWN,
        Rune::CheckCircle => &CHECK_CIRCLE,
        Rune::XCircle => &X_CIRCLE,
        Rune::HandRaised => &MESSAGE_QUESTION_SQUARE,
        Rune::VideoCamera => &VIDEO_RECORDER,
        Rune::Refresh => &REFRESH_CW_01,
        Rune::PhoneCall => &PHONE_CALL_01,
        Rune::Webcam => &WEBCAM_01,
        Rune::ExternalDisplay => &MONITOR_05,
    }
}

pub(super) fn draw(scene: &mut Scene<'_>, b: Rect, fg: Color, rune: Rune) {
    art(rune).draw(scene, b, fg, INK_FILL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::art::tests::assert_fits;

    const ALL: [Rune; 33] = [
        Rune::Headphones,
        Rune::Speaker,
        Rune::Laptop,
        Rune::Display,
        Rune::Keyboard,
        Rune::Gamepad,
        Rune::Phone,
        Rune::Microphone,
        Rune::Sun,
        Rune::Moon,
        Rune::Wifi,
        Rune::Ethernet,
        Rune::Bluetooth,
        Rune::Warning,
        Rune::GaugeLow,
        Rune::GaugeMid,
        Rune::GaugeHigh,
        Rune::Window,
        Rune::Workspace,
        Rune::Sparkles,
        Rune::Stopwatch,
        Rune::Clock,
        Rune::MusicalNote,
        Rune::CommandLine,
        Rune::ArrowDownTray,
        Rune::CheckCircle,
        Rune::XCircle,
        Rune::HandRaised,
        Rune::VideoCamera,
        Rune::Refresh,
        Rune::PhoneCall,
        Rune::Webcam,
        Rune::ExternalDisplay,
    ];

    #[test]
    fn every_rune_parses_inside_the_view_box() {
        for rune in ALL {
            assert_fits(art(rune), &format!("{rune:?}"));
        }
    }
}
