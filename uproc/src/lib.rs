
use std::any::*;
use std::sync::{Arc, RwLock};
use crossbeam::channel::{Sender, Receiver};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

pub type Pid = usize;

// A message in the system, consisting of the sender PID and the actual message contents
pub type Msg = (Pid, Box<dyn Any + Send + Sync>);

/// A process context from which processes can spawn other processes or send messages
#[derive(Clone)]
pub struct Context {
    self_pid: Pid,
    inj: Arc<crossbeam::deque::Injector<ProcessTask>>,
    rx: Receiver<Msg>,
    next_pid: Arc<AtomicUsize>,
    process_senders: Arc<RwLock<BTreeMap<Pid, Sender<Msg>>>>
}

impl Context {
    /// Spawn a process, returns the process id
    pub fn spawn(&self, p: impl Process + Send + Sync + 'static) -> Pid {
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst); //could this ordering be relaxed?
        let (tx, rx) = crossbeam::channel::unbounded::<Msg>();
        self.inj.push(ProcessTask{
            pid,
            code: Box::new(p),
            rx
        });
        self.process_senders.write().unwrap().insert(pid, tx.clone());
        pid
    }

    /// Send a message to a process, blocking 
    pub fn send(&self, to_pid: Pid, msg: impl Any + Send + Sync) {
        // println!("send {} -> {}", self.self_pid, to_pid);
        self.process_senders.read().unwrap().get(&to_pid).expect("pid is valid")
            .send((self.self_pid, Box::new(msg)));
    }

    /// Recieve a message send to this process
    pub fn recv(&self) -> Msg {
        self.rx.recv().unwrap()
    }
}

/// The state of a process in the scheduler
pub enum ProcessState {
    /// The process is still waiting to recieve messages
    Waiting,
    /// The process has finished and no longer needs to be scheduled
    Finished
}

/// A process that can be executed
pub trait Process {
    /// Process a message from `sender` The context `cx` is for this process
    /// Return the new state of the process after processing the message
    fn process_message(&mut self, cx: &mut Context, sender: Pid, msg: &dyn Any) -> ProcessState;
}

impl<T> Process for T where T: FnMut(&mut Context, Pid, &dyn Any)->ProcessState {
    fn process_message(&mut self, cx: &mut Context, sender: Pid, msg: &dyn Any) -> ProcessState {
        (self)(cx, sender, msg)
    }
}

struct ProcessTask {
    pid: Pid,
    code: Box<dyn Process + Send + Sync>,
    rx: Receiver<Msg>
}

/// The top level process scheduler, which is cooperative
pub struct Scheduler {
    injector: Arc<crossbeam::deque::Injector<ProcessTask>>,
    next_pid: Arc<AtomicUsize>,
    process_senders: Arc<RwLock<BTreeMap<Pid, Sender<Msg>>>>,
    main_rx: Receiver<Msg>,
}

fn find_task(global: &crossbeam::deque::Injector<ProcessTask>,
             local: &crossbeam::deque::Worker<ProcessTask>,
             stealers: &[crossbeam::deque::Stealer<ProcessTask>]) -> Option<ProcessTask>
{
    // from the crossbeam docs
    local.pop().or_else(|| {
        std::iter::repeat_with(|| {
            global.steal_batch_and_pop(local)
                .or_else(|| stealers.iter().map(|s| s.steal()).collect())
        })
        .find(|s| !s.is_retry())
        .and_then(|s| s.success())
    })
}

impl Scheduler {
    /// Create a new scheduler that can run `count` processes in parallel
    pub fn with_threads(count: usize) -> Scheduler {
        let injector = Arc::new(crossbeam::deque::Injector::new());
        let process_senders = Arc::new(RwLock::new(BTreeMap::new()));
        let next_pid = Arc::new(AtomicUsize::new(1));
        let (main_tx, main_rx) = crossbeam::channel::unbounded();
        process_senders.write().unwrap().insert(0, main_tx);
        use crossbeam::deque::{Worker, Stealer};
        let (mut work_qus, stealers): (Vec<Option<Worker<ProcessTask>>>, Vec<Stealer<ProcessTask>>) = (0..count).map(|_| {
            let wk = Worker::new_fifo();
            let st = wk.stealer();
            (Some(wk), st)
        }).unzip();
        for wq in work_qus.iter_mut() {
            let wrk_qu = wq.take().unwrap();
            let inj = injector.clone();
            let stl = stealers.clone();
            let npid = next_pid.clone();
            let psen = process_senders.clone();
            std::thread::spawn(move || {
                loop {
                    if let Some(mut task) = find_task(inj.as_ref(), &wrk_qu, &stl) {
                        match task.rx.try_recv() {
                            Ok((pid, msg)) => {
                                let mut cx = Context {
                                    self_pid: task.pid,
                                    inj: inj.clone(),
                                    rx: task.rx.clone(),
                                    next_pid: npid.clone(),
                                    process_senders: psen.clone()
                                };
                                match task.code.process_message(&mut cx, pid, msg.as_ref()) {
                                    ProcessState::Waiting => wrk_qu.push(task),
                                    ProcessState::Finished => {}
                                }
                            },
                            Err(crossbeam::channel::TryRecvError::Empty) => {
                                wrk_qu.push(task);
                            },
                            Err(_) => {}
                        }
                    }
                }
            });
        }
        Scheduler {
            injector,
            main_rx,
            process_senders,
            next_pid
        }
    }

    /// Create a new scheduler with one worker thread per logical CPU
    pub fn new() -> Scheduler { Scheduler::with_threads(num_cpus::get()) }

    /// Get the context for the main thread, so that it can send/recv messages and spawn processes
    pub fn main_context(&self) -> Context {
        Context {
            self_pid: 0,
            inj: self.injector.clone(),
            rx: self.main_rx.clone(),
            next_pid: self.next_pid.clone(),
            process_senders: self.process_senders.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let schd = Scheduler::with_threads(1);
        let cx = schd.main_context();
        let p = cx.spawn(move |cx: &mut Context, _, msg: &dyn Any| {
            assert_eq!(msg.downcast_ref::<u32>(), Some(&42));
            cx.send(0, msg.downcast_ref::<u32>().cloned().map(|i| i + 3).unwrap());
            ProcessState::Finished
        });
        cx.send(p, 42u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(45));
    }

    #[test]
    fn two_procs() {
        let schd = Scheduler::with_threads(2);
        let cx = schd.main_context();
        let proc1 = cx.spawn(move |cx: &mut Context, sender: Pid, msg: &dyn Any| {
            let i = msg.downcast_ref::<u32>().cloned().unwrap();
            println!("1: {}", i);
            if i > 10 { cx.send(0, i); return ProcessState::Finished; }
            cx.send(sender, i + 1);
            ProcessState::Waiting
        });
        let proc2 = cx.spawn(move |cx: &mut Context, _: Pid, msg: &dyn Any| {
            let i = msg.downcast_ref::<u32>().cloned().unwrap();
            println!("2: {}", i);
            if i > 10 { return ProcessState::Finished; }
            cx.send(proc1, i + 1);
            ProcessState::Waiting
        });
        cx.send(proc2, 0u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(11));
    }
}
