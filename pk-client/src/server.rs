use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use futures::prelude::*;
use pk_common::protocol;
use super::Error;

struct FutureResponse {
    msg_id: protocol::MessageId,
    aio: nng::Aio,
    responses: Arc<Mutex<HashMap<protocol::MessageId, protocol::Response>>>
}

impl Future for FutureResponse {
    type Output = protocol::Response;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut futures::task::Context) -> futures::task::Poll<Self::Output> {
        use futures::task::Poll;
        //println!("polling for {:?}", self.msg_id);
        cx.waker().wake_by_ref();
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
    next_msg_id: protocol::MessageId
}

impl Server {
    fn process(aio: &nng::Aio, cx: &nng::Context, responses: &Arc<Mutex<HashMap<protocol::MessageId, protocol::Response>>>, res: nng::AioResult) {
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
                let resp: protocol::MsgResponse = match serde_cbor::from_slice(m.as_slice()) {
                    Ok(r) => r,
                    Err(e) => { println!("error decoding response: {}", e); return; }
                };
                let mut r = responses.lock().unwrap();
                r.insert(resp.req_id, resp.msg);
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

    pub fn init(url: &str) -> Result<Server, Error> {
        let socket = nng::Socket::new(nng::Protocol::Req0).map_err(Error::from_other)?;

        let responses = Arc::new(Mutex::new(HashMap::new()));

        socket.dial(url).map_err(Error::from_other)?;

        Ok(Server {
            responses, socket, next_msg_id: protocol::MessageId(1)
        })
    }

    pub fn request(&mut self, msg: protocol::Request) -> impl Future<Output=protocol::Response> {
        let mut wmsg = nng::Message::new().unwrap();
        let msg_id = self.next_msg_id;
        serde_cbor::to_writer(&mut wmsg, &protocol::MsgRequest { msg_id, msg }).unwrap();
        let cx = nng::Context::new(&self.socket).map_err(Error::from_other).unwrap();
        let context = cx.clone();
        let resp = self.responses.clone();
        let aio = nng::Aio::new(move |aio, res| Server::process(&aio, &cx, &resp, res)).unwrap();
        context.send(&aio, wmsg).unwrap();
        self.next_msg_id = protocol::MessageId(self.next_msg_id.0 + 1);
        FutureResponse { msg_id, responses: self.responses.clone(), aio }
    }
}
