use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::live::wire::{AgentEvent, AgentReport, AgentTool, ToolVerb};

use crate::{
    live::{
        Accent, Activity, Caption, Control, Feed, Glyph, LiveSource, Origin, Signal, Steer, Urgency,
    },
    util::{deadline::sleep_until, proc},
};

/// How long "done" stays on the chip before it goes.
const DONE_LINGER: Duration = Duration::from_secs(8);
const FAILED_LINGER: Duration = Duration::from_secs(30);
/// How often sessions are checked for an agent that died without saying so.
const PRUNE_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct AgentSource {
    sessions: Vec<Session>,
}

struct Session {
    id: Arc<str>,
    tool: AgentTool,
    project: Arc<str>,
    pid: u32,
    phase: Phase,
    /// When the current turn began.
    since: Instant,
    /// When the phase last changed, for lingering.
    changed: Instant,
    dismissed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Thinking,
    Tool { verb: ToolVerb, subject: Arc<str> },
    Waiting { reason: Arc<str> },
    Compacting,
    Done,
    Failed { reason: Arc<str> },
}

impl Phase {
    fn busy(&self) -> bool {
        !matches!(self, Self::Idle | Self::Done | Self::Failed { .. })
    }

    fn linger(&self) -> Option<Duration> {
        match self {
            Self::Done => Some(DONE_LINGER),
            Self::Failed { .. } => Some(FAILED_LINGER),
            _ => None,
        }
    }
}

impl Session {
    /// Move to the phase `event` names. Returns `false` once the session is
    /// over.
    fn hear(&mut self, event: AgentEvent, now: Instant) -> bool {
        let phase = match event {
            AgentEvent::Opened => Phase::Idle,
            AgentEvent::Prompted => {
                self.since = now;
                self.dismissed = false;
                Phase::Thinking
            }
            AgentEvent::Tool { verb, subject } => Phase::Tool {
                verb,
                subject: Arc::from(subject),
            },
            AgentEvent::ToolDone => Phase::Thinking,
            AgentEvent::Waiting { reason } => Phase::Waiting {
                reason: Arc::from(reason),
            },
            AgentEvent::Compacting => Phase::Compacting,
            AgentEvent::Finished if self.phase.busy() => Phase::Done,
            AgentEvent::Finished => Phase::Idle,
            AgentEvent::Failed { reason } => Phase::Failed {
                reason: Arc::from(reason),
            },
            AgentEvent::Closed => return false,
        };
        if !self.phase.busy() && phase.busy() {
            self.since = now;
        }
        if phase != self.phase {
            self.phase = phase;
            self.changed = now;
        }
        true
    }

    fn expired(&self, now: Instant) -> bool {
        self.phase
            .linger()
            .is_some_and(|linger| self.changed + linger <= now)
    }

    fn deadline(&self) -> Option<Instant> {
        self.phase.linger().map(|linger| self.changed + linger)
    }

    fn shown(&self) -> bool {
        !self.dismissed && self.phase != Phase::Idle
    }

    fn activity(&self, feed: &Feed<AgentReport>, now: Instant) -> Activity {
        let name = self.tool.name();
        let base = Activity::new(feed.key(self.id.clone()), Glyph::Sparkles, name)
            .accent(self.tool.accent())
            .caption(Caption::Since(self.since))
            .detail("Agent", name)
            .detail("Project", self.project.clone())
            .controls([Control::Dismiss]);
        match &self.phase {
            Phase::Idle | Phase::Thinking => base.headline("Thinking"),
            Phase::Tool { verb, subject } => base.headline(format!("{} {subject}", verb.gerund())),
            Phase::Waiting { reason } => base
                .headline("Needs input")
                .glyph(Glyph::Hand)
                .accent(Accent::Amber)
                .urgency(Urgency::Attention)
                .detail("Request", reason.clone()),
            Phase::Compacting => base.headline("Compacting"),
            Phase::Done => base
                .headline("Done")
                .glyph(Glyph::Check)
                .urgency(Urgency::Ambient)
                .caption(Caption::Frozen(
                    self.changed.saturating_duration_since(self.since),
                )),
            Phase::Failed { reason } => base
                .headline("Failed")
                .glyph(Glyph::Cross)
                .accent(Accent::Red)
                .caption(Caption::Frozen(now.saturating_duration_since(self.since)))
                .detail("Reason", reason.clone()),
        }
    }
}

fn project(cwd: &str) -> Arc<str> {
    Arc::from(
        Path::new(cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(cwd),
    )
}

impl AgentSource {
    fn hear(&mut self, report: AgentReport, now: Instant) -> Option<Arc<str>> {
        let index = match self
            .sessions
            .iter()
            .position(|session| *session.id == *report.session)
        {
            Some(index) => index,
            None => {
                self.sessions.push(Session {
                    id: Arc::from(report.session),
                    tool: report.tool,
                    project: project(&report.cwd),
                    pid: report.pid,
                    phase: Phase::Idle,
                    since: now,
                    changed: now,
                    dismissed: false,
                });
                self.sessions.len() - 1
            }
        };
        let session = &mut self.sessions[index];
        session.pid = report.pid;
        if !report.cwd.is_empty() {
            session.project = project(&report.cwd);
        }
        match session.hear(report.event, now) {
            true => None,
            false => Some(self.sessions.remove(index).id),
        }
    }

    fn dismiss(&mut self, id: &str) {
        if let Some(session) = self.sessions.iter_mut().find(|session| &*session.id == id) {
            session.dismissed = true;
        }
    }

    /// Settle lingering phases, forget agents that are gone, and show what is
    /// left.
    fn publish(&mut self, feed: &Feed<AgentReport>, now: Instant) {
        self.sessions.retain(|session| {
            let alive = proc::alive(session.pid);
            if !alive {
                feed.retire(session.id.clone());
            }
            alive
        });
        for session in &mut self.sessions {
            if session.expired(now) {
                session.phase = Phase::Idle;
            }
            match session.shown() {
                true => feed.show(session.activity(feed, now)),
                false => feed.retire(session.id.clone()),
            }
        }
    }

    fn deadline(&self, now: Instant) -> Option<Instant> {
        let prune = (!self.sessions.is_empty()).then(|| now + PRUNE_EVERY);
        self.sessions
            .iter()
            .filter_map(Session::deadline)
            .chain(prune)
            .min()
    }
}

impl LiveSource for AgentSource {
    type Input = AgentReport;
    const ORIGIN: Origin = Origin::Agent;

    async fn run(mut self, mut feed: Feed<Self::Input>) {
        loop {
            let deadline = self.deadline(Instant::now());
            tokio::select! {
                signal = feed.next() => match signal {
                    Some(Signal::Input(report)) => {
                        if let Some(closed) = self.hear(report, Instant::now()) {
                            feed.retire(closed);
                        }
                    }
                    Some(Signal::Steer(Steer::Control(id, Control::Dismiss))) => self.dismiss(&id),
                    Some(Signal::Steer(_)) => continue,
                    None => return,
                },
                _ = sleep_until(deadline) => {}
            }
            self.publish(&feed, Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(event: AgentEvent) -> AgentReport {
        AgentReport {
            tool: AgentTool::ClaudeCode,
            session: "s1".into(),
            pid: std::process::id(),
            cwd: "/e/CrownOS/crownbar".into(),
            event,
        }
    }

    #[test]
    fn a_turn_runs_through_tools_and_waits_then_finishes() {
        let start = Instant::now();
        let later = |seconds| start + Duration::from_secs(seconds);
        let mut source = AgentSource::default();
        source.hear(report(AgentEvent::Opened), start);
        assert!(!source.sessions[0].shown());

        source.hear(report(AgentEvent::Prompted), later(1));
        source.hear(
            report(AgentEvent::Tool {
                verb: ToolVerb::Edit,
                subject: "bar.rs".into(),
            }),
            later(5),
        );
        let session = &source.sessions[0];
        assert_eq!(session.since, later(1));
        assert_eq!(&*session.project, "crownbar");

        source.hear(
            report(AgentEvent::Waiting {
                reason: "Bash".into(),
            }),
            later(6),
        );
        assert!(matches!(source.sessions[0].phase, Phase::Waiting { .. }));
        source.hear(report(AgentEvent::ToolDone), later(9));
        source.hear(report(AgentEvent::Finished), later(20));
        assert_eq!(source.sessions[0].phase, Phase::Done);
        assert!(source.sessions[0].expired(later(20) + DONE_LINGER));

        assert_eq!(
            source
                .hear(report(AgentEvent::Closed), later(30))
                .as_deref(),
            Some("s1")
        );
        assert!(source.sessions.is_empty());
    }

    #[test]
    fn a_stop_with_no_turn_running_shows_nothing() {
        let now = Instant::now();
        let mut source = AgentSource::default();
        source.hear(report(AgentEvent::Finished), now);
        assert_eq!(source.sessions[0].phase, Phase::Idle);
    }
}
