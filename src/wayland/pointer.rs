use smithay_client_toolkit::{
    delegate_pointer,
    seat::pointer::{PointerEvent, PointerEventKind, PointerHandler},
    shell::WaylandSurface,
};
use wayland_client::{Connection, QueueHandle, protocol::wl_pointer};

use crate::window::Window;

/// Linux input-event-codes BTN_LEFT.
const BTN_LEFT: u32 = 0x110;

impl PointerHandler for Window {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let our_surface = self.layer.wl_surface().clone();
        for event in events {
            if event.surface != our_surface {
                continue;
            }
            let (x, y) = event.position;
            match event.kind {
                PointerEventKind::Enter { .. } => self.on_pointer_enter(x, y),
                PointerEventKind::Leave { .. } => self.on_pointer_leave(),
                PointerEventKind::Motion { .. } => self.on_pointer_motion(x, y),
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    self.on_pointer_click(x, y)
                }
                _ => {}
            }
        }
    }
}

delegate_pointer!(Window);
