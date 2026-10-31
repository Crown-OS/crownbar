use crownui::{
    kit::{Theme, provide_theme},
    prelude::{Cx, Runtime, Signal},
};

/// Provides the current scope with a theme that follows `derive`, so the kit
/// widgets built inside it restyle whenever what it reads changes.
pub fn provide_derived_theme(
    cx: &mut Cx,
    derive: impl Fn(&mut Runtime) -> Theme + 'static,
) -> Signal<Theme> {
    let theme = provide_theme(cx, Theme::DEFAULT);
    cx.effect(move |runtime| {
        let derived = derive(runtime);
        theme.set(runtime, derived);
    });
    theme
}
