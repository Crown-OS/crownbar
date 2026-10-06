//! The surface's size, as a signal the panel reads to stay on screen.

use crownui::{
    ext::{Kind, NodeId},
    prelude::{Cx, Signal, Size},
};

pub struct SizeProbeKind {
    target: Signal<Size>,
}

impl Kind for SizeProbeKind {
    const NAME: &'static str = "size_probe";

    fn mount(self, cx: &mut Cx, node: NodeId) {
        let observed = cx.observe_size(node);
        let target = self.target;
        cx.effect(move |runtime| {
            let size = observed.get(runtime);
            target.set(runtime, size);
        });
    }
}

/// Follows the size of whatever box it is given — the whole surface, here.
pub fn size_probe(target: Signal<Size>) -> crownui::prelude::Element<SizeProbeKind> {
    crownui::prelude::Element::new(SizeProbeKind { target })
}
