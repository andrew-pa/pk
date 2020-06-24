use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use futures::prelude::*;
use pk_common::protocol;
use super::Error;

struct FutureResponse {
    msg_id: protocol::MessageId,
    aio: nng::Aio,
    wakers: Arc<Mutex<HashMap<protocol::MessageId, futures::task::Waker>>>,
    responses: Arc<Mutex<HashMap<protocol::MessageId, protocol::Response>>>
}

impl Future for FutureResponse {
    type Output = protocol::Response;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut futures::task::Context) -> futures::task::Poll<Self::Output> {
        use futures::task::Poll;
        // println!("polling for {:?}", self.msg_id);
        { self.wakers.lock().unwrap().insert(self.msg_id, cx.waker().to_owned()); }
        // TODO: there might be an interesting condition here where the response for the request comes in
        // before poll() ever gets called, in which case the waker will just get left in the
        // hashmap forever. Probably not a good idea to leak wakers.
        match self.responses.try_lock() {
            Ok(mut responses) => {
                responses.remove(&self.msg_id).map_or(Poll::Pending, |r| { Poll::Ready(r) })
            },
            Err(std::sync::TryLockError::WouldBlock) => { Poll::Pending },
            Err(e) => { println!("{}",e); Poll::Pending }
        }
    }
}

pub struct Server {
    socket: nng::Socket,
    responses: Arc<Mutex<HashMap<protocol::MessageId, protocol::Response>>>,
    wakers: Arc<Mutex<HashMap<protocol::MessageId, futures::task::Waker>>>,
    next_msg_id: protocol::MessageId,
    thread_pool: futures::executor::ThreadPool
}

impl Server {
    fn process(aio: &nng::Aio, cx: &nng::Context,
               responses: &Arc<Mutex<HashMap<protocol::MessageId, protocol::Response>>>,
               wakers: &Arc<Mutex<HashMap<protocol::MessageId, futures::task::Waker>>>,
               thread_pool: &futures::executor::ThreadPool,
               res: nng::AioResult) {
        use nng::AioResult;
        println!("process {:?}", res);
        match res {
            AioResult::Send(Ok(_)) => loop {
                match cx.recv(aio) {
                    Ok(()) => break,
                    Err(nng::Error::TryAgain) => continue,
                    Err(e) => { println!("error trying to recv {}", e); break; }
                }
            },
            AioResult::Recv(Ok(m)) => {
                let responses = responses.clone();
                let wakers = wakers.clone();
                // run this on the thread pool to escape the NNG worker thread's small stack.
                // Unneccessary on optimized builds, but it's probably good to keep as much
                // computation out of the Aio handler as possible
                thread_pool.spawn_ok(async move {
                    let resp: protocol::MsgResponse = match serde_cbor::from_slice(m.as_slice()) {
                        Ok(r) => r,
                        Err(e) => { println!("error decoding response: {}", e); return; }
                    };
                    {
                        let mut r = responses.lock().unwrap();
                        r.insert(resp.req_id, resp.msg);
                    }
                    {
                        let mut w = wakers.lock().unwrap();
                        match w.remove(&resp.req_id).map(|w| w.wake()) {
                            Some(()) => {},
                            None => println!("waker for {:?} missing", resp.req_id)
                        }
                    }
                });
            },
            AioResult::Send(Err((_,e)))  => {
                println!("error in nng AIO, send! {:?}", e);
            }
            AioResult::Recv(Err(e)) => {
                println!("error in nng AIO, recv! {:?}", e);
            },
            _ => panic!("unexpected AioResult")
        }
    }

    pub fn init(url: &str, thread_pool: futures::executor::ThreadPool) -> Result<Server, Error> {
        let socket = nng::Socket::new(nng::Protocol::Req0).map_err(Error::from_other)?;

        let responses = Arc::new(Mutex::new(HashMap::new()));
        let wakers = Arc::new(Mutex::new(HashMap::new()));

        socket.dial(url).map_err(Error::from_other)?;

        Ok(Server {
            responses, wakers, socket, next_msg_id: protocol::MessageId(1),
            thread_pool
        })
    }

    pub fn request(&mut self, msg: protocol::Request) -> impl Future<Output=protocol::Response> {
        let mut wmsg = nng::Message::new().unwrap();
        let msg_id = self.next_msg_id;
        serde_cbor::to_writer(&mut wmsg, &protocol::MsgRequest { msg_id, msg }).unwrap();
        let cx = nng::Context::new(&self.socket).map_err(Error::from_other).unwrap();
        let context = cx.clone();
        let resp = self.responses.clone();
        let waks = self.wakers.clone();
        let tp = self.thread_pool.clone();
        let aio = nng::Aio::new(move |aio, res| Server::process(&aio, &cx, &resp, &waks, &tp, res)).unwrap();
        context.send(&aio, wmsg).unwrap();
        self.next_msg_id = protocol::MessageId(self.next_msg_id.0 + 1);
        FutureResponse { msg_id, responses: self.responses.clone(), wakers: self.wakers.clone(), aio }
    }
}
