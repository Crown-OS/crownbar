use smithay_client_toolkit::{
    delegate_output,
    output::{OutputHandler, OutputState},
};
use wayland_client::{Connection, QueueHandle, protocol::wl_output};

use crate::window::Window;

impl OutputHandler for Window {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

delegate_output!(Window);
