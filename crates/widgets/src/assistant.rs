use std::sync::Arc;

use crate::{
    AfterAction, BarWidget, Icon, OrbMood, PopupAction, PopupSpec,
    popup::{Header, PanelBuilder, Row},
};
use crownbar_integrations::{
    Services,
    assistant::{AssistantCommand, AssistantSnapshot},
    live::Accent,
};
use suja_proto::AssistantState;

const LEVEL_FOLLOW: f32 = 12.0;
const LEVEL_GAIN: f32 = 6.0;

pub struct AssistantWidget {
    snapshot: Arc<AssistantSnapshot>,
    phase: f32,
    level: f32,
    targets: Vec<Option<AssistantCommand>>,
}

impl AssistantWidget {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(AssistantSnapshot::default()),
            phase: 0.0,
            level: 0.0,
            targets: Vec::new(),
        }
    }

    fn mood(&self) -> OrbMood {
        match &self.snapshot.state {
            AssistantState::Idle => OrbMood::Idle,
            AssistantState::Listening => OrbMood::Listening,
            AssistantState::Thinking | AssistantState::Confirming => OrbMood::Thinking,
            AssistantState::Speaking => OrbMood::Speaking,
            AssistantState::Tool { .. } => OrbMood::Tool,
        }
    }

    fn active(&self) -> bool {
        self.snapshot.state.is_active()
    }
}

impl Default for AssistantWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl BarWidget for AssistantWidget {
    fn id(&self) -> &str {
        "assistant"
    }

    fn sync(&mut self, services: &Services) -> bool {
        let next = services.assistant.read();
        if Arc::ptr_eq(&self.snapshot, &next) {
            return false;
        }
        self.snapshot = next;
        true
    }

    fn visible(&self) -> bool {
        self.snapshot.connected
    }

    fn label(&self) -> &str {
        match &self.snapshot.state {
            AssistantState::Idle => "",
            AssistantState::Listening => "Listening",
            AssistantState::Thinking => "Thinking",
            AssistantState::Confirming => "Confirm?",
            AssistantState::Speaking => "Speaking",
            AssistantState::Tool { name } => name,
        }
    }

    fn icon(&self) -> Icon {
        Icon::Orb {
            level: self.level,
            phase: self.phase,
            mood: self.mood(),
        }
    }

    fn accent(&self) -> Option<Accent> {
        self.active().then_some(Accent::Violet)
    }

    fn tick_animation(&mut self, dt: f32) -> bool {
        let target = if self.active() {
            (self.snapshot.level * LEVEL_GAIN).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.level += (target - self.level) * (dt * LEVEL_FOLLOW).min(1.0);
        if self.active() {
            self.phase = (self.phase + dt).rem_euclid(1_000.0);
            return true;
        }
        if self.level > 0.005 || self.phase != 0.0 {
            self.phase = 0.0;
            return true;
        }
        self.level = 0.0;
        false
    }

    fn popup(&mut self, _services: &Services) -> Option<PopupSpec> {
        let snapshot = &self.snapshot;
        let mut panel = PanelBuilder::new();
        panel.row(Header::new("Suja").row());
        panel.row(Row::Readout {
            primary: if snapshot.heard.is_empty() {
                "Hold the hotkey and speak".to_owned()
            } else {
                snapshot.heard.clone()
            },
            secondary: snapshot.reply.clone(),
        });
        if let Some(tool) = &snapshot.tool {
            let secondary = match &tool.outcome {
                Some((true, summary)) => summary.clone(),
                Some((false, summary)) => format!("failed: {summary}"),
                None => "running".to_owned(),
            };
            panel.row(Row::Readout {
                primary: tool.name.clone(),
                secondary,
            });
        }
        if let Some(ms) = snapshot.last_first_audio_ms {
            panel.row(Row::Readout {
                primary: format!("{ms} ms"),
                secondary: "from your last word to the first spoken one".to_owned(),
            });
        }
        panel.row(Row::Separator);
        panel.action(
            Row::action(if snapshot.state == AssistantState::Listening {
                "Stop listening"
            } else {
                "Talk"
            }),
            AssistantCommand::TalkToggle,
        );
        if snapshot.state.is_active() {
            panel.action(Row::action("Cancel"), AssistantCommand::Cancel);
        }
        let (spec, targets) = panel.finish();
        self.targets = targets;
        Some(spec)
    }

    fn on_popup(&mut self, action: PopupAction, services: &Services) -> AfterAction {
        let PopupAction::Activate { row } = action else {
            return AfterAction::Stay;
        };
        match self.targets.get(row).copied().flatten() {
            Some(command) => {
                services.assistant.send(command);
                AfterAction::Stay
            }
            None => AfterAction::Stay,
        }
    }

    fn popup_poll(&mut self, _slow: bool) -> bool {
        true
    }
}
