//! The two snapshots a device listing leaves out.
//!
//! crownlink's `state` hands back a topic exactly as the phone published it,
//! postcard-encoded as llts-signaling's document for that topic. Only the
//! leading fields the bar reads are mirrored here; postcard ignores what
//! follows. Anything that does not decode is simply not known yet — the next
//! event will say.

use crownlink_linux::ipc::proto::CallInfo;
use crownos_ipc::postcard;
use serde::Deserialize;

/// llts-signaling's `Hotspot { enabled, ssid }`.
#[derive(Deserialize)]
struct Hotspot {
    enabled: bool,
}

/// llts-signaling's `CallState { calls }`, whose calls are field for field
/// crownlink's own [`CallInfo`].
#[derive(Deserialize)]
struct Calls {
    calls: Vec<CallInfo>,
}

pub(super) fn hotspot(document: Option<&[u8]>) -> Option<bool> {
    let hotspot: Hotspot = postcard::from_bytes(document?).ok()?;
    Some(hotspot.enabled)
}

pub(super) fn calls(document: Option<&[u8]>) -> Vec<CallInfo> {
    document
        .and_then(|bytes| postcard::from_bytes::<Calls>(bytes).ok())
        .map(|state| state.calls)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crownlink_linux::ipc::proto::CallStatus;

    use super::*;

    #[test]
    fn documents_decode_as_the_phone_publishes_them() {
        let published = postcard::to_stdvec(&(true, Some("Pixel hotspot"))).unwrap_or_default();
        assert_eq!(hotspot(Some(&published)), Some(true));

        let call = CallInfo {
            call_id: "7".to_owned(),
            number: "123".to_owned(),
            contact_name: Some("Mum".to_owned()),
            status: CallStatus::Active,
            answered_unix_ms: Some(1),
        };
        let published = postcard::to_stdvec(&(vec![call.clone()],)).unwrap_or_default();
        assert_eq!(calls(Some(&published)), [call]);
    }

    #[test]
    fn an_unknown_or_garbled_topic_is_not_known() {
        assert_eq!(hotspot(None), None);
        assert_eq!(hotspot(Some(&[])), None);
        assert!(calls(Some(&[0xff, 0xff])).is_empty());
    }
}
