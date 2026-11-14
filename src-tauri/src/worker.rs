//! One thread owns the USB connection and runs jobs strictly in the order they were sent.
//! The UI thread never touches the device, and two quick clicks can't reach the mouse
//! out of order.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;

use crate::device::Mouse;
use crate::error::Error;
use crate::hidpp::Transport;

pub type Opener = fn() -> Result<Box<dyn Transport>, Error>;
type Job = Box<dyn FnOnce(&mut Session) + Send>;

/// The worker's state: an open connection, or none until the next job needs one.
pub struct Session {
    open: Opener,
    mouse: Option<Mouse<Box<dyn Transport>>>,
}

impl Session {
    /// Returns the connected mouse, connecting first if needed.
    pub fn mouse(&mut self) -> Result<&mut Mouse<Box<dyn Transport>>, Error> {
        if self.mouse.is_none() {
            self.mouse = Some(Mouse::new((self.open)()?));
        }
        Ok(self.mouse.as_mut().unwrap())
    }

    fn disconnect(&mut self) {
        self.mouse = None;
    }
}

pub struct Worker {
    jobs: mpsc::Sender<Job>,
}

impl Worker {
    pub fn spawn(open: Opener) -> Worker {
        let (jobs, inbox) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("g502-usb".into())
            .spawn(move || {
                let mut session = Session { open, mouse: None };
                for job in inbox {
                    // A bug in one job must not kill the worker; start the next job fresh.
                    if catch_unwind(AssertUnwindSafe(|| job(&mut session))).is_err() {
                        session.disconnect();
                    }
                }
            })
            .expect("failed to start the USB worker thread");
        Worker { jobs }
    }

    /// Runs `f` on the worker with a connected mouse and waits for its result.
    pub async fn run<R, F>(&self, f: F) -> Result<R, Error>
    where
        R: Send + 'static,
        F: FnOnce(&mut Mouse<Box<dyn Transport>>) -> Result<R, Error> + Send + 'static,
    {
        let (reply, answer) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move |session| {
            let result = session.mouse().and_then(|m| {
                m.hidpp.drain()?;
                f(m)
            });
            if matches!(&result, Err(e) if e.drops_connection()) {
                session.disconnect();
            }
            let _ = reply.send(result);
        });
        self.jobs
            .send(job)
            .map_err(|_| Error::Connection("the USB worker stopped".into()))?;
        answer
            .await
            .map_err(|_| Error::Connection("the USB worker stopped mid-job".into()))?
    }
}
