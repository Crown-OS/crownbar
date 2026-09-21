//! Proves the idle-inhibit binding against whatever compositor is running.
//!
//! Creates a layer surface, holds an inhibitor, holds it again, releases it,
//! and reports what the compositor accepted. A protocol error would tear the
//! connection down and `run` would return `Err`, so reaching the end is the
//! result.

use std::{cell::Cell, rc::Rc, time::Duration};

use crownshell::{
    calloop, Anchor, KeyboardInteractivity, Layer, Scene, SurfaceCtx, SurfaceHandler, WindowConfig,
};

struct Probe;

impl SurfaceHandler for Probe {
    fn paint(&mut self, _scene: &mut Scene, _ctx: SurfaceCtx<'_>) {}
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let step = Rc::new(Cell::new(0u8));

    crownshell::run(move |app| {
        app.create_window(
            WindowConfig {
                namespace: "crownshell-idle-probe".into(),
                layer: Layer::Top,
                anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                size: (0, 8),
                keyboard_interactivity: KeyboardInteractivity::None,
                blur: false,
                ..Default::default()
            },
            Probe,
        );
        println!("supports_idle_inhibit = {}", app.supports_idle_inhibit());

        let step = step.clone();
        app.loop_handle().insert_source(
            calloop::timer::Timer::from_duration(Duration::from_millis(500)),
            move |_, _, app| {
                match step.get() {
                    0 => {
                        let ok = app.set_idle_inhibited(true);
                        println!("hold  -> accepted={ok} inhibited={}", app.is_idle_inhibited());
                    }
                    1 => {
                        // Idempotent: asking again must not create a second
                        // object or drop the first.
                        let ok = app.set_idle_inhibited(true);
                        println!("hold again -> accepted={ok} inhibited={}", app.is_idle_inhibited());
                    }
                    2 => {
                        app.set_idle_inhibited(false);
                        println!("release -> inhibited={}", app.is_idle_inhibited());
                    }
                    _ => {
                        println!("no protocol error; connection still alive");
                        app.exit = true;
                        return calloop::timer::TimeoutAction::Drop;
                    }
                }
                step.set(step.get() + 1);
                calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
            },
        )
        .map_err(|e| anyhow::anyhow!("could not register the probe timer: {}", e.error))?;
        Ok(())
    })
}
