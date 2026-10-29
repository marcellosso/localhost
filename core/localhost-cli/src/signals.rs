//! Being told to end: Ctrl-C, or the terminal or session going away. Once
//! these are watched, none of them ends the process on its own: it can stop
//! what it started first.

#[cfg(unix)]
use tokio::signal::unix::{Signal, SignalKind, signal};
#[cfg(windows)]
use tokio::signal::windows::{CtrlBreak, CtrlC, CtrlClose, ctrl_break, ctrl_c, ctrl_close};

/// SIGINT (Ctrl-C), SIGTERM, and SIGHUP (the terminal closed).
#[cfg(unix)]
pub struct Signals([Signal; 3]);

/// Ctrl-C, Ctrl-Break, and the console closing.
#[cfg(windows)]
pub struct Signals(CtrlC, CtrlBreak, CtrlClose);

impl Signals {
    #[cfg(unix)]
    pub fn new() -> std::io::Result<Signals> {
        Ok(Signals([signal(SignalKind::interrupt())?, signal(SignalKind::terminate())?, signal(SignalKind::hangup())?]))
    }

    #[cfg(windows)]
    pub fn new() -> std::io::Result<Signals> {
        Ok(Signals(ctrl_c()?, ctrl_break()?, ctrl_close()?))
    }

    #[cfg(unix)]
    pub async fn next(&mut self) {
        let [interrupt, terminate, hang_up] = &mut self.0;
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
            _ = hang_up.recv() => {}
        }
    }

    #[cfg(windows)]
    pub async fn next(&mut self) {
        let Signals(interrupt, interrupt_hard, close) = self;
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = interrupt_hard.recv() => {}
            _ = close.recv() => {}
        }
    }
}
