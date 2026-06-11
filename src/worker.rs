use tokio::sync::{mpsc, oneshot};
type Job = Box<dyn FnOnce() + Send>;

#[derive(Clone)]
pub struct Worker {
    tx: mpsc::UnboundedSender<Job>,
}

impl Worker {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Job>();
        std::thread::Builder::new()
            .name("cu-automation".into())
            .spawn(move || {
                use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
                use windows::Win32::UI::HiDpi::{
                    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                };
                unsafe {
                    let _ =
                        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                }
                while let Some(job) = rx.blocking_recv() {
                    job();
                }
            })
            .expect("spawn automation worker");
        Worker { tx }
    }

    /// Run a blocking closure on the automation thread and await its result.
    pub async fn run<R, F>(&self, f: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (otx, orx) = oneshot::channel();
        if self
            .tx
            .send(Box::new(move || {
                let _ = otx.send(f());
            }))
            .is_err()
        {
            panic!("automation worker thread is gone");
        }
        orx.await.expect("automation worker dropped the job")
    }
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}
