pub(crate) struct BroadcastChannel<T> {
    pub notify: async_std::channel::Sender<T>,
    pub notified: async_std::channel::Receiver<T>,
}

impl<T: Clone> BroadcastChannel<T> {
    pub fn channel() -> Self {
        let (notify, notified) = async_std::channel::bounded(1);
        Self { notify, notified }
    }
}

impl<T: Clone> Clone for BroadcastChannel<T> {
    fn clone(&self) -> Self {
        Self {
            notify: self.notify.clone(),
            notified: self.notified.clone(),
        }
    }
}
