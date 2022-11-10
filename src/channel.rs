use tokio::sync::broadcast;

pub struct BroadcastChannel<T> {
    pub notify: broadcast::Sender<T>,
    pub notified: broadcast::Receiver<T>,
}

impl<T: Clone> BroadcastChannel<T> {
    pub fn channel() -> Self {
        let (notify, notified) = broadcast::channel(1);
        Self { notify, notified }
    }
}

impl<T: Clone> Clone for BroadcastChannel<T> {
    fn clone(&self) -> Self {
        Self {
            notify: self.notify.clone(),
            notified: self.notify.subscribe(),
        }
    }
}
