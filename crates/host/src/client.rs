//! How a front end talks to the host: submit a request, poll for the answer,
//! watch progress, cancel. The in-process client runs the host on its own
//! thread, so a front end never blocks, and simulation uses a thread pool of
//! (cores - 1) so the machine stays usable.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use calibration::Calibration;
use storage::LocalStore;

use crate::api::{Progress, Request, Response};
use crate::session::{Control, Host, saves_of};

/// Names one submitted request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticket(pub u64);

/// What a front end needs from a league host, in-process or remote.
pub trait GameClient {
    /// Send a request; returns at once.
    fn submit(&self, req: Request) -> Ticket;
    /// The answer, once there is one (and only once).
    fn poll(&self, t: Ticket) -> Option<Response>;
    /// How far the running request has got.
    fn progress(&self) -> Progress;
    /// Ask the running request to stop at the next safe point.
    fn cancel(&self);

    /// Send a request and wait for its answer.
    fn call(&self, req: Request) -> Response {
        let t = self.submit(req);
        loop {
            if let Some(r) = self.poll(t) {
                return r;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// A host on its own thread in this process.
pub struct LocalClient {
    tx: Option<Sender<(u64, Request)>>,
    rx: Mutex<Receiver<(u64, Response)>>,
    answers: Mutex<BTreeMap<u64, Response>>,
    next: AtomicU64,
    control: Arc<Control>,
    worker: Option<JoinHandle<()>>,
}

impl LocalClient {
    /// Start a host. `saves` is the folder for save files (none: saving is
    /// refused).
    #[must_use]
    pub fn start(cal: Calibration, saves: Option<PathBuf>) -> Self {
        let (tx, jobs) = channel::<(u64, Request)>();
        let (out, rx) = channel::<(u64, Response)>();
        let control = Arc::new(Control::default());
        let ctl = Arc::clone(&control);
        let worker = std::thread::Builder::new()
            .name("league-host".into())
            .spawn(move || run(&cal, saves, &ctl, &jobs, &out))
            .ok();
        Self {
            tx: Some(tx),
            rx: Mutex::new(rx),
            answers: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            control,
            worker,
        }
    }
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .saturating_sub(1)
        .max(1)
}

fn run(
    cal: &Calibration,
    saves: Option<PathBuf>,
    ctl: &Arc<Control>,
    jobs: &Receiver<(u64, Request)>,
    out: &Sender<(u64, Response)>,
) {
    // Simulation work spreads over (cores - 1) threads; the first host in the
    // process sets the size of the shared pool.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads())
        .thread_name(|i| format!("sim-{i}"))
        .build_global();
    let open = || saves.as_ref().and_then(|d| LocalStore::open(d).ok());
    let mut host: Option<Host> = None;
    while let Ok((id, req)) = jobs.recv() {
        let resp = {
            match (&mut host, req.clone()) {
                (_, Request::NewGame(g)) => match Host::new_game(cal, &g) {
                    Ok(mut h) => {
                        h.set_saves(host.as_mut().and_then(Host::take_saves).or_else(open));
                        h.control = Arc::clone(ctl);
                        host = Some(h);
                        Response::Done("New league founded".into())
                    }
                    Err(e) => Response::Failed(e),
                },
                (_, Request::Load { slot }) => match open() {
                    None => Response::Failed("no save folder".into()),
                    Some(store) => match Host::from_save(cal, &store, &slot) {
                        Ok(mut h) => {
                            h.set_saves(Some(store));
                            h.control = Arc::clone(ctl);
                            host = Some(h);
                            Response::Done("Loaded".into())
                        }
                        Err(e) => Response::Failed(e),
                    },
                },
                (None, Request::ListSaves) => match open() {
                    Some(store) => saves_of(&store),
                    None => Response::Saves(Vec::new()),
                },
                (Some(h), r) => h.handle(r),
                (None, _) => Response::Failed("no game in progress".into()),
            }
        };
        if out.send((id, resp)).is_err() {
            break;
        }
    }
}

impl GameClient for LocalClient {
    fn submit(&self, req: Request) -> Ticket {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let sent = self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.send((id, req)).is_ok());
        if let (false, Ok(mut a)) = (sent, self.answers.lock()) {
            a.insert(id, Response::Failed("the league host has stopped".into()));
        }
        Ticket(id)
    }

    fn poll(&self, t: Ticket) -> Option<Response> {
        if let (Ok(rx), Ok(mut answers)) = (self.rx.lock(), self.answers.lock()) {
            while let Ok((id, r)) = rx.try_recv() {
                answers.insert(id, r);
            }
            return answers.remove(&t.0);
        }
        None
    }

    fn progress(&self) -> Progress {
        self.control
            .progress
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    fn cancel(&self) {
        self.control
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Drop for LocalClient {
    fn drop(&mut self) {
        self.cancel();
        self.tx = None;
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
