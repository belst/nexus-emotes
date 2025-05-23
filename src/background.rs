use std::sync::mpsc;
use std::thread;

type Job = Box<dyn FnOnce() + Send>;

pub struct Worker {
    input_queue: Option<mpsc::Receiver<Job>>,
    tx: Option<mpsc::Sender<Job>>,
    thread: Option<thread::JoinHandle<()>>,
}

pub struct RunningWorker {
    worker: Worker,
}

impl Worker {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            input_queue: Some(rx),
            tx: Some(tx),
            thread: None,
        }
    }

    pub fn run(mut self) -> RunningWorker {
        let rx = self.input_queue.take().expect("Queue to exist");
        let thread = thread::Builder::new()
            .name("Background Worker".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    log::trace!("Received job");
                    job();
                    log::trace!("Finished job");
                }
                log::trace!("Worker thread exiting");
            })
            .unwrap();
        self.thread = Some(thread);
        RunningWorker { worker: self }
    }
}

impl RunningWorker {
    pub fn spawn(&self, job: Job) {
        if let Some(tx) = self.worker.tx.as_ref() {
            tx.send(job).unwrap();
        }
    }

    pub fn join(self) {}
}

impl Drop for RunningWorker {
    fn drop(&mut self) {
        drop(self.worker.tx.take());
        if let Some(t) = self.worker.thread.take() {
            t.join().unwrap();
        }
    }
}
