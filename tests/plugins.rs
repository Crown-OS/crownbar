//! Plugin pills against a stand-in crownplugind: the tree arrives, its panel
//! opens, what the user does goes back tagged with the tree it was done to,
//! and the pills leave with the daemon. Then `bar.ron` is edited under the
//! running bar, which rearranges itself.
//!
//! Its own test binary, and one `#[test]`, because it points the process at a
//! private runtime and config directory before anything reads them.

use std::{
    any::Any,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use crownbar::{
    BarSettings,
    services::{Services, Wake},
};
use crownconfig::Appearance;
use crownos_ipc::{RemoteError, Server, ServiceBuilder};
use crownplugin_proto::{
    Decision, EntityInfo, Host, LogLine, Node, NodeId, Part, RemoteSurface, SettingValue, Snapshot,
    Source, SurfaceKey, Tree, UiEvent, plugind,
};
use crowntest::{Role, TestApp, role};
use view::{TaskExecutor, TaskFuture, TaskId};

const ENTRY: &str = "plugin:test.cpu/status";
const SURFACE: (f32, f32) = (1440.0, 900.0);

#[derive(Debug, Clone, PartialEq)]
enum Heard {
    Ui(SurfaceKey, u32, UiEvent),
    Popup(Part, bool),
}

#[derive(Default)]
struct Daemon {
    revision: u64,
    heard: Arc<Mutex<Vec<Heard>>>,
    changed: Option<Snapshot>,
    stop: bool,
}

impl Daemon {
    fn hear(&self, heard: Heard) {
        if let Ok(mut log) = self.heard.lock() {
            log.push(heard);
        }
    }

    fn snapshot(&self) -> Snapshot {
        let reading = format!("CPU {}%", 40 + self.revision);
        Snapshot {
            revision: self.revision,
            surfaces: vec![RemoteSurface {
                plugin: "test.cpu".into(),
                widget: "status".into(),
                main: Some(Tree {
                    rev: self.revision as u32,
                    root: Node::Text {
                        style: Vec::new(),
                        text: reading,
                    },
                }),
                popup: Some(Tree {
                    rev: self.revision as u32,
                    root: Node::VStack {
                        style: Vec::new(),
                        children: vec![Node::Toggle {
                            id: NodeId(7),
                            label: "Turbo".into(),
                            on: false,
                        }],
                    },
                }),
                lock_slot: None,
            }],
        }
    }
}

fn unsupported<T>() -> Result<T, RemoteError> {
    Err(RemoteError::handler("not in this test"))
}

impl plugind::Handler for Daemon {
    fn attach(&mut self, host: Host) -> Result<Snapshot, RemoteError> {
        assert_eq!(host, Host::Bar);
        Ok(self.snapshot())
    }

    fn ui_event(&mut self, surface: SurfaceKey, rev: u32, event: UiEvent) {
        self.hear(Heard::Ui(surface, rev, event));
        self.revision += 1;
        self.changed = Some(self.snapshot());
    }

    fn popup(&mut self, surface: SurfaceKey, open: bool) {
        self.hear(Heard::Popup(surface.part, open));
    }

    fn list(&mut self) -> Result<Vec<EntityInfo>, RemoteError> {
        unsupported()
    }

    fn install(&mut self, _source: Source) -> Result<String, RemoteError> {
        unsupported()
    }

    fn uninstall(&mut self, _id: String) -> Result<(), RemoteError> {
        unsupported()
    }

    fn set_enabled(&mut self, _id: String, _enabled: bool) -> Result<(), RemoteError> {
        unsupported()
    }

    fn set_theme(&mut self, _id: Option<String>) -> Result<(), RemoteError> {
        unsupported()
    }

    fn set_grant(
        &mut self,
        _id: String,
        _capability: String,
        _decision: Option<Decision>,
    ) -> Result<(), RemoteError> {
        unsupported()
    }

    fn settings(&mut self, _id: String) -> Result<Vec<(String, SettingValue)>, RemoteError> {
        unsupported()
    }

    fn set_setting(
        &mut self,
        _id: String,
        _key: String,
        _value: SettingValue,
    ) -> Result<(), RemoteError> {
        unsupported()
    }

    fn logs(&mut self, _id: String) -> Result<Vec<LogLine>, RemoteError> {
        unsupported()
    }

    fn dev_load(&mut self, _path: String) -> Result<String, RemoteError> {
        unsupported()
    }

    /// Stands in for the daemon going away.
    fn reload(&mut self, _id: String) -> Result<(), RemoteError> {
        self.stop = true;
        Ok(())
    }
}

/// Serves `daemon` until something asks it to `reload`.
fn serve(mut server: Server, mut daemon: Daemon) {
    let served = crownos_ipc::blocking::serve(&mut server, |server, peer, message| {
        plugind::dispatch(&mut daemon, server, peer, message)?;
        if let Some(snapshot) = daemon.changed.take() {
            server.emit(&plugind::BarChanged { snapshot })?;
        }
        Ok(!daemon.stop)
    });
    served.expect("the stand-in daemon serves");
}

type Finished = (TaskId, Box<dyn Any + Send>);

/// Runs the bar's `cx.spawn` futures, which a headless app leaves unstarted,
/// and hands back what they finish with.
struct Tasks {
    runtime: tokio::runtime::Runtime,
    finished: Sender<Finished>,
}

impl TaskExecutor for Tasks {
    fn spawn(&mut self, id: TaskId, future: TaskFuture) {
        let finished = self.finished.clone();
        self.runtime.spawn(async move {
            let _ = finished.send((id, future.await));
        });
    }

    fn cancel(&mut self, _id: TaskId) {}
}

fn run_tasks(app: &mut TestApp) -> Receiver<Finished> {
    let (finished, outputs) = channel();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("task runtime");
    app.ui()
        .set_task_executor(Box::new(Tasks { runtime, finished }));
    outputs
}

/// A private home for everything the bar and the daemon touch.
fn isolate() -> PathBuf {
    let root = PathBuf::from(format!("/tmp/cbp-{}", std::process::id()));
    for (variable, directory) in [
        ("XDG_RUNTIME_DIR", "run"),
        ("CROWN_CONFIG_DIR", "config"),
        ("CROWN_DATA_DIR", "data"),
        ("CROWN_STATE_DIR", "state"),
    ] {
        let path = root.join(directory);
        std::fs::create_dir_all(&path).expect("temp dir");
        // SAFETY: nothing else runs yet; this is the test binary's only test,
        // and it isolates itself before starting a thread.
        unsafe { std::env::set_var(variable, &path) };
    }
    unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent") };
    root
}

fn eventually(
    app: &mut TestApp,
    tasks: &Receiver<Finished>,
    what: &str,
    mut done: impl FnMut(&mut TestApp) -> bool,
) {
    for _ in 0..200 {
        while let Ok((id, output)) = tasks.try_recv() {
            app.ui().complete_task(id, output);
        }
        app.settle();
        if done(app) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("never saw {what}:\n{}", app.snapshot());
}

#[test]
fn a_plugin_pill_follows_its_daemon() {
    let root = isolate();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let daemon = Daemon {
        revision: 1,
        heard: Arc::clone(&heard),
        ..Daemon::default()
    };
    let server = ServiceBuilder::new(plugind::SERVICE)
        .build()
        .expect("socket binds");
    let daemon = thread::spawn(move || serve(server, daemon));

    let settings = BarSettings {
        left: vec!["clock".into()],
        right: vec![ENTRY.into(), "no-such-widget".into()],
        ..BarSettings::default()
    };
    let wake = Wake::default();
    let services = Services::start(wake.clone()).expect("services start");
    let mut app = TestApp::with_size(
        crownbar::view(services, wake, Appearance::default(), settings),
        SURFACE.0,
        SURFACE.1,
    );

    let tasks = run_tasks(&mut app);
    let shows =
        |reading: &'static str| move |app: &mut TestApp| app.frame_snapshot().contains(reading);

    eventually(&mut app, &tasks, "the plugin's pill", shows("CPU 41%"));
    assert!(app.exists(role(Role::Button).name(ENTRY)));
    assert!(
        !app.exists(role(Role::Button).name("battery")),
        "a built-in left out of bar.ron is hidden"
    );

    app.click(role(Role::Button).name(ENTRY));
    eventually(&mut app, &tasks, "the plugin's panel", |app| {
        app.exists(role(Role::Switch).name("Turbo"))
    });
    app.click(role(Role::Switch).name("Turbo"));
    eventually(&mut app, &tasks, "the next tree", shows("CPU 42%"));

    app.pointer_down(SURFACE.0 - 20.0, SURFACE.1 - 20.0)
        .pointer_up(SURFACE.0 - 20.0, SURFACE.1 - 20.0);
    eventually(&mut app, &tasks, "the panel go away", |app| {
        !app.exists(role(Role::Switch).name("Turbo"))
    });
    eventually(&mut app, &tasks, "the daemon hear it", |_| {
        heard.lock().is_ok_and(|heard| heard.len() >= 3)
    });

    let popup = SurfaceKey {
        plugin: "test.cpu".into(),
        widget: "status".into(),
        part: Part::Popup,
    };
    assert_eq!(
        *heard.lock().expect("not poisoned"),
        [
            Heard::Popup(Part::Popup, true),
            Heard::Ui(popup, 1, UiEvent::Toggle(NodeId(7), true)),
            Heard::Popup(Part::Popup, false),
        ]
    );

    // The daemon goes before its reply is flushed, so the answer is a
    // disconnect as often as not.
    let _ = plugind::Client::connect().and_then(|mut client| {
        let pending = client.reload("everything".into())?;
        client.wait(pending, Some(Duration::from_secs(5)))
    });
    daemon.join().expect("the daemon thread ends");
    eventually(&mut app, &tasks, "the pill leave with its daemon", |app| {
        !app.exists(role(Role::Button).name(ENTRY))
    });
    assert!(app.exists(role(Role::Button).name("clock")));

    let edited = r#"(left: [], center: ["clock"], right: ["layout"])"#;
    std::fs::write(root.join("config/bar.ron"), edited).expect("bar.ron written");
    eventually(&mut app, &tasks, "the bar rearranged", |app| {
        app.exists(role(Role::Button).name("layout"))
    });
    let clock = app.find(role(Role::Button).name("clock")).bounds;
    let centre = clock.min_x() + clock.size.width / 2.0;
    assert!(
        (centre - SURFACE.0 / 2.0).abs() < 1.0,
        "the clock moved to the centre, at {centre}"
    );

    std::fs::remove_dir_all(root).ok();
}
