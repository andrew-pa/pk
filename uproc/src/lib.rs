//! A message passing based cooperative lightweight process scheduler. 
//!
//! # Example
//! ```rust
//! let schd = Scheduler::new();  // create a new scheduler
//! let cx = schd.main_context(); // get the context for the main thread
//! // spawn a process that just sends back a message and exits
//! let proc = cx.spawn(move |cx: &mut Context, sender: Pid, msg: &dyn Any| {
//!     cx.send(sender, msg);
//!     Ok(ProcessState::Finished)
//! });
//! cx.send(proc, 5u32); // send proc a 5
//! assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(5)); // get the 5 back
//! ```

use std::any::*;
use std::sync::{Arc, RwLock};
use crossbeam::channel::{Sender, Receiver};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use futures::prelude::*;

pub type Pid = usize;

/// A message in the system, consisting of the sender PID and the actual message contents
pub type Msg = (Pid, Box<dyn Any + Send>);

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
    /// If `supervise` is true, then when this process exits either normally or by an error, the
    /// `ProcessResult` will be sent back to this process as a message from the spawned process
    pub fn spawn_sup(&self, p: impl Process + Send + 'static, supervise: bool) -> Pid {
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst); //could this ordering be relaxed?
        let (tx, rx) = crossbeam::channel::unbounded::<Msg>();
        self.inj.push(ProcessTask {
            pid,
            code: Box::new(p),
            rx,
            supv: if supervise { Some(self.self_pid) } else { None }
        });
        self.process_senders.write().unwrap().insert(pid, tx.clone());
        pid
    }
    
    /// Spawn an unsupervised process
    pub fn spawn(&self, p: impl Process + Send + 'static) -> Pid {
        self.spawn_sup(p, false)
    }

    /// Spawn a future on the scheduler and run it to completion asynchronously
    pub fn spawn_future<F>(&self, fut: F) -> Pid
        where F: Future<Output=()> + Send + 'static
    {
        let pid = self.spawn(FuturePollOnRecv { fut, send_out: false });
        self.send(pid, ());
        pid
    }

    /// Spawn a future on the scheduler that will send a message back of type `Out` when it is finished
    pub fn future_message<Out: Send + 'static, F>(&self, fut: F) -> Pid
        where F: Future<Output=Out> + Send + 'static
    {
        let pid = self.spawn(FuturePollOnRecv { fut, send_out: true });
        self.send(pid, ());
        pid
    }

    /// Send a message to a process. Does block, but should finish quickly
    pub fn send(&self, to_pid: Pid, msg: impl Any + Send) {
        // println!("send {} -> {}", self.self_pid, to_pid);
        self.process_senders.read().unwrap().get(&to_pid).expect("pid is valid")
            .send((self.self_pid, Box::new(msg))).unwrap();
    }

    /// Send a message to a process, pretending to be from `from_pid`, blocking 
    fn send_to_self(&self, from_pid: Pid, msg: impl Any + Send) {
        // println!("send {} -> {}", self.self_pid, to_pid);
        self.process_senders.read().unwrap().get(&self.self_pid).expect("pid is valid")
            .send((from_pid, Box::new(msg))).unwrap();
    }

    /// Recieve a message send to this process, or block until one is sent
    pub fn recv(&self) -> Msg {
        self.rx.recv().unwrap()
    }

    /// Try to recieve a message send to this process, or return `None`
    pub fn try_recv(&self) -> Option<Msg> {
        match self.rx.try_recv() {
            Ok(m) => Some(m),
            Err(crossbeam::channel::TryRecvError::Empty) => None,
            Err(e) => panic!(e)
        }
    }

    pub fn pid(&self) -> Pid { self.self_pid }

    // TODO: it seems reasonable to have an async `recv` function, but you'd need to keep track of
    // all the wakers for the different futures, and there would need to be a policy about how
    // multiple messages are mapped to futures (ie first come first serve, single outstanding only)
}

/// The state of a process in the scheduler
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessState {
    /// The process is still waiting to recieve messages
    Waiting,
    /// The process has finished and no longer needs to be scheduled
    Finished,
}

pub type ProcessResult = Result<ProcessState, usize>;

/// A process that can be executed
pub trait Process {
    /// Process a message from `sender` The context `cx` is for this process. This will only be
    /// called if a process recieves messages, so if the process never recieves any messages, it
    /// will never run.
    /// Return the new state of the process after processing the message or an error code
    fn process_message(&mut self, cx: &mut Context, sender: Pid, msg: &dyn Any) -> ProcessResult;
}

impl<T> Process for T where T: FnMut(&mut Context, Pid, &dyn Any)->ProcessResult {
    fn process_message(&mut self, cx: &mut Context, sender: Pid, msg: &dyn Any) -> ProcessResult {
        (self)(cx, sender, msg)
    }
}

struct FuturePollOnRecv<Out: Send + 'static, F: Future<Output=Out>> {
    fut: F,
    send_out: bool
}

struct FuturePollOnRecvWaker {
    cx: Context,
    target: Pid,
}

impl futures::task::ArcWake for FuturePollOnRecvWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // replay the same message that started the future, so that the interested sending process
        // can recieve the result
        arc_self.cx.send_to_self(arc_self.target, arc_self.clone());
    }
}

impl <Out: Send + 'static, F: Future<Output=Out>> Process for FuturePollOnRecv<Out, F> {
    fn process_message(&mut self, cx: &mut Context, sender: Pid, msg: &dyn Any) -> ProcessResult {
        // use futures::task::{Poll, Context};
        // let fut = &mut self.fut;
        // let wak = futures::task::waker(msg.downcast_ref::<Arc<FuturePollOnRecvWaker>>()
        //     .map_or_else(|| Arc::new(FuturePollOnRecvWaker{cx: cx.clone(), target: sender}), Clone::clone));
        // match Future::poll(std::pin::Pin::new(fut), &mut Context::from_waker(&wak)) {
        //     Poll::Pending => Ok(ProcessState::Waiting),
        //     Poll::Ready(v) => {
        //         if self.send_out { cx.send(sender, v); }
        //         Ok(ProcessState::Finished)
        //     }
        // }
        unimplemented!();
    }
}

struct ProcessTask {
    pid: Pid,
    code: Box<dyn Process + Send>,
    rx: Receiver<Msg>,
    supv: Option<Pid>
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
    // notably this will just keep cycling through all the waiting tasks and not ever get new ones.
    // This seems like it could be reasonable, since if all the tasks are waiting, there's no
    // reason to go try to steal new tasks to wait on
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
                                    Ok(ProcessState::Waiting) => wrk_qu.push(task),
                                    state => {
                                        if let Some(spid) = task.supv {
                                            cx.send(spid, state);
                                        }
                                    }
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
            Ok(ProcessState::Finished)
        }, false);
        cx.send(p, 42u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(45));
    }

    #[test]
    fn simple_supv() {
        let schd = Scheduler::with_threads(1);
        let cx = schd.main_context();
        let p = cx.spawn(move |cx: &mut Context, _, msg: &dyn Any| {
            assert_eq!(msg.downcast_ref::<u32>(), Some(&42));
            cx.send(0, msg.downcast_ref::<u32>().cloned().map(|i| i + 3).unwrap());
            Ok(ProcessState::Finished)
        }, true);
        cx.send(p, 42u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(45));
        assert_eq!(cx.recv().1.downcast_ref::<ProcessResult>(), Some(&Ok(ProcessState::Finished)));
    }

    #[test]
    fn simple_err_supv() {
        let schd = Scheduler::with_threads(1);
        let cx = schd.main_context();
        let p = cx.spawn(move |cx: &mut Context, _, msg: &dyn Any| {
            assert_eq!(msg.downcast_ref::<u32>(), Some(&42));
            cx.send(0, msg.downcast_ref::<u32>().cloned().map(|i| i + 3).unwrap());
            Err(53)
        }, true);
        cx.send(p, 42u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(45));
        assert_eq!(cx.recv().1.downcast_ref::<ProcessResult>(), Some(&Err(53)));
    }

    #[test]
    fn two_procs() {
        let schd = Scheduler::with_threads(2);
        let cx = schd.main_context();
        let proc1 = cx.spawn(move |cx: &mut Context, sender: Pid, msg: &dyn Any| {
            let i = msg.downcast_ref::<u32>().cloned().unwrap();
            // println!("1: {}", i);
            if i > 10 { cx.send(0, i); return Ok(ProcessState::Finished); }
            cx.send(sender, i + 1);
            Ok(ProcessState::Waiting)
        }, false);
        let proc2 = cx.spawn(move |cx: &mut Context, _: Pid, msg: &dyn Any| {
            let i = msg.downcast_ref::<u32>().cloned().unwrap();
            // println!("2: {}", i);
            if i > 10 { return Ok(ProcessState::Finished); }
            cx.send(proc1, i + 1);
            Ok(ProcessState::Waiting)
        }, false);
        cx.send(proc2, 0u32);
        assert_eq!(cx.recv().1.downcast_ref::<u32>().cloned(), Some(11));
    }

    #[test]
    fn lots_of_procs() {
        let schd = Scheduler::with_threads(8);
        let cx = schd.main_context();
        let mut processes = Vec::new();
        for _ in 0..100 {
            let mut sum = 0;
            processes.push(cx.spawn(move |cx: &mut Context, sender: Pid, msg: &dyn Any| {
                match msg.downcast_ref::<u32>() {
                    Some(0) => {
                        //println!("exit!");
                        assert_eq!(sum, 3);
                        cx.send(sender, cx.pid());
                        Ok(ProcessState::Finished)
                    },
                    Some(c) => {
                        //println!("{} got {} {}", i, c, sum);
                        sum += c;
                        Ok(ProcessState::Waiting)
                    },
                    None => Ok(ProcessState::Waiting)
                }
            }, false));
        }
        for x in 1..3 {
            for p in processes.iter() {
                //println!("sending {} -> {}", x, *p);
                cx.send(*p, x as u32);
            }
        }
        for p in processes.iter() {
            cx.send(*p, 0u32);
        }
        for _ in 0..1000 {
            let (sp, pid) = cx.recv();
            let pid = pid.downcast_ref::<Pid>().unwrap();
            assert_eq!(sp, *pid);
            for i in 0..processes.len() {
                if processes[i] == *pid {
                    processes.remove(i);
                    break;
                }
            }
            if processes.len() == 0 { break; }
        }
        assert_eq!(processes.len(), 0);
    }

    #[test]
    fn run_future_to_end() {
        let schd = Scheduler::with_threads(2);
        let cx = schd.main_context();
        let future_ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fr = future_ran.clone();
        cx.spawn_future(futures::future::lazy(move |_| {
            fr.store(true, std::sync::atomic::Ordering::Relaxed);
        }), false);
        for _ in 0..100000 {
            if future_ran.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
        }
        panic!("future never ran!");
    }

    #[test]
    fn run_future_message() {
        let schd = Scheduler::with_threads(2);
        let cx = schd.main_context();
        let pid = cx.future_message(futures::future::lazy(|_| {
            42u32
        }), false);
        let (rpid, m) = cx.recv();
        assert_eq!(rpid, pid);
        assert_eq!(m.downcast_ref::<u32>(), Some(&42));
    }
}
