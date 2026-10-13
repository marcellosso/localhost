//! Event streams the app drains with `await next()`. `close()` ends a
//! pending `next()` too, which is how a Swift `AsyncStream` that is torn
//! down stops listening.

use std::sync::Arc;

use tokio::sync::{Mutex, mpsc, watch};

use super::runtime::on_runtime;
use crate::{OutputLine, ProjectLibraryEvent, ServiceEvent, ServiceOutputLine};

macro_rules! subscription {
    ($(#[$doc:meta])* $name:ident, $item:ty) => {
        $(#[$doc])*
        #[derive(uniffi::Object)]
        pub struct $name {
            receiver: Mutex<mpsc::UnboundedReceiver<$item>>,
            closed: watch::Sender<bool>,
        }

        impl $name {
            pub(super) fn new(receiver: mpsc::UnboundedReceiver<$item>) -> Arc<$name> {
                Arc::new($name { receiver: Mutex::new(receiver), closed: watch::Sender::new(false) })
            }
        }

        #[uniffi::export]
        impl $name {
            /// The next item, or `None` once the stream is closed or its
            /// source is gone.
            pub async fn next(self: Arc<Self>) -> Option<$item> {
                on_runtime(async move {
                    let mut closed = self.closed.subscribe();
                    let mut receiver = self.receiver.lock().await;
                    tokio::select! {
                        item = receiver.recv() => item,
                        _ = closed.wait_for(|closed| *closed) => None,
                    }
                })
                .await
            }

            /// Stops the stream; a pending and every later `next()` return `None`.
            pub fn close(&self) {
                self.closed.send_replace(true);
            }
        }
    };
}

subscription!(
    /// Changes to the Project library.
    LibraryEvents,
    ProjectLibraryEvent
);
subscription!(
    /// Service State changes and Runs starting and ending.
    ServiceEvents,
    ServiceEvent
);
subscription!(
    /// Lines appended to one Run.
    OutputLines,
    OutputLine
);
subscription!(
    /// Lines appended to any Run of a set of Services.
    ServiceOutputLines,
    ServiceOutputLine
);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{Id, Origin};

    #[tokio::test]
    async fn delivers_items_then_ends_when_the_source_is_gone() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let events = LibraryEvents::new(receiver);
        let id = Id::new();
        sender.send(ProjectLibraryEvent::ProjectRemoved { id, origin: Origin::Cli }).unwrap();
        drop(sender);
        assert_eq!(events.clone().next().await, Some(ProjectLibraryEvent::ProjectRemoved { id, origin: Origin::Cli }));
        assert_eq!(events.next().await, None);
    }

    #[tokio::test]
    async fn close_ends_a_pending_next() {
        let (_sender, receiver) = mpsc::unbounded_channel::<ServiceEvent>();
        let events = ServiceEvents::new(receiver);
        let pending = tokio::spawn(events.clone().next());
        tokio::time::sleep(Duration::from_millis(50)).await;
        events.close();
        assert_eq!(tokio::time::timeout(Duration::from_secs(1), pending).await.unwrap().unwrap(), None);
        assert_eq!(events.next().await, None);
    }
}
