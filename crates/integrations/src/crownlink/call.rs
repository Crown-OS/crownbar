//! The one call a device is in.
//!
//! crownlink can report the same call twice: once from the phone over llts and
//! once from the Bluetooth hands-free link, under two different ids. So a call
//! is keyed by its device, never by its id, and each id it has been reported
//! under is remembered so that whichever of them ends it, ends it.

use std::{sync::Arc, time::Instant};

use crownlink_linux::ipc::proto::{CallInfo, CallStatus};

use crate::util::epoch::instant_at;

/// How many trailing digits two numbers must share to be the same line, so
/// `+44 7700 900123` and `07700900123` are one caller.
const NUMBER_TAIL: usize = 9;

/// A call that has been answered. A ringing call is crownotify's to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneCall {
    /// Every id the call was reported under, newest last.
    ids: Vec<Arc<str>>,
    pub number: Arc<str>,
    pub contact: Option<Arc<str>>,
    pub held: bool,
    /// When it was answered, so its clock survives a reconnect.
    pub answered: Instant,
}

impl PhoneCall {
    /// The contact's name, or the number when the phone does not know it.
    pub fn caller(&self) -> &str {
        self.contact.as_deref().unwrap_or(&self.number)
    }

    /// The id to hang the call up by. crownlink accepts either form.
    pub fn id(&self) -> &str {
        self.ids.last().map_or("", |id| &**id)
    }

    fn new(info: &CallInfo, now: Instant) -> Self {
        Self {
            ids: vec![Arc::from(info.call_id.as_str())],
            number: Arc::from(info.number.as_str()),
            contact: info.contact_name.as_deref().map(Arc::from),
            held: info.status == CallStatus::Held,
            answered: info.answered_unix_ms.map_or(now, instant_at),
        }
    }

    fn describes(&self, info: &CallInfo) -> bool {
        self.ids.iter().any(|id| **id == *info.call_id) || same_line(&self.number, &info.number)
    }

    fn merge(&mut self, info: &CallInfo) {
        if !self.ids.iter().any(|id| **id == *info.call_id) {
            self.ids.push(Arc::from(info.call_id.as_str()));
        }
        if let Some(contact) = &info.contact_name {
            self.contact = Some(Arc::from(contact.as_str()));
        }
        self.held = info.status == CallStatus::Held;
    }
}

/// The call a device is in once `info` is taken into account.
pub(super) fn follow(call: &mut Option<PhoneCall>, info: &CallInfo, now: Instant) {
    let ongoing = matches!(info.status, CallStatus::Active | CallStatus::Held);
    match call {
        Some(held) if held.describes(info) => match ongoing {
            true => held.merge(info),
            false if info.status == CallStatus::Ended => *call = None,
            false => {}
        },
        Some(_) if info.status != CallStatus::Active => {}
        _ if ongoing => *call = Some(PhoneCall::new(info, now)),
        _ => {}
    }
}

/// Whether two numbers reach the same line. Two withheld numbers are the same
/// line; a withheld one is no other.
fn same_line(left: &str, right: &str) -> bool {
    let digits = |number: &str| -> Vec<u8> {
        number
            .bytes()
            .filter(u8::is_ascii_digit)
            .rev()
            .take(NUMBER_TAIL)
            .collect()
    };
    digits(left) == digits(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, number: &str, status: CallStatus) -> CallInfo {
        CallInfo {
            call_id: id.to_owned(),
            number: number.to_owned(),
            contact_name: None,
            status,
            answered_unix_ms: None,
        }
    }

    #[test]
    fn ringing_and_dialing_are_not_calls_yet() {
        let mut call = None;
        let now = Instant::now();
        follow(&mut call, &info("1", "123", CallStatus::Ringing), now);
        follow(&mut call, &info("1", "123", CallStatus::Dialing), now);
        assert_eq!(call, None);
        follow(&mut call, &info("1", "123", CallStatus::Active), now);
        assert_eq!(call.as_ref().map(PhoneCall::caller), Some("123"));
    }

    #[test]
    fn a_call_reported_under_two_ids_is_one_call_ended_by_either() {
        let start = Instant::now();
        let mut call = None;
        follow(
            &mut call,
            &info("llts-7", "+447700900123", CallStatus::Active),
            start,
        );
        let later = start + std::time::Duration::from_secs(5);
        follow(
            &mut call,
            &info("/org/ofono/call1", "07700900123", CallStatus::Active),
            later,
        );
        let held = call.as_ref().map(|call| (call.answered, call.id()));
        assert_eq!(held, Some((start, "/org/ofono/call1")));
        follow(&mut call, &info("llts-7", "", CallStatus::Ended), later);
        assert_eq!(call, None);
    }

    #[test]
    fn a_waiting_call_neither_displaces_nor_ends_the_one_in_progress() {
        let now = Instant::now();
        let mut call = None;
        for waiting in ["222222222", ""] {
            follow(&mut call, &info("a", "111111111", CallStatus::Active), now);
            follow(&mut call, &info("b", waiting, CallStatus::Ringing), now);
            follow(&mut call, &info("b", waiting, CallStatus::Held), now);
            follow(&mut call, &info("b", waiting, CallStatus::Ended), now);
            assert_eq!(call.as_ref().map(PhoneCall::id), Some("a"));
        }
    }

    #[test]
    fn answering_a_second_line_moves_the_call_to_it() {
        let now = Instant::now();
        let mut call = None;
        follow(&mut call, &info("a", "111111111", CallStatus::Active), now);
        follow(&mut call, &info("b", "222222222", CallStatus::Active), now);
        assert_eq!(call.as_ref().map(PhoneCall::id), Some("b"));
    }
}
