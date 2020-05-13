
use pk_common::*;
use pk_common::protocol;

use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::sync::mpsc::{channel,Sender};

#[derive(Debug)]
enum ServerError { 
    MessageSerdeError(serde_cbor::Error),
    TransportError(nng::Error),
    IoError(std::io::Error),
    UnknownMessage
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MessageSerdeError(e) => write!(f, "error serializing/deserializing message: {}", e),
            Self::TransportError(e) => write!(f, "error in transport: {}", e),
            Self::IoError(e) => write!(f, "io error: {}", e),
            Self::UnknownMessage => write!(f, "unrecongized message recieved"),
            _ => write!(f, "job error")
        }
    }
}

impl std::error::Error for ServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MessageSerdeError(e) => Some(e),
            Self::TransportError(e) => Some(e),
            Self::IoError(e) => Some(e),
            _ => None
        }
    }
}


fn make_error_message(e: ServerError) -> nng::Message {
    let mut msg = nng::Message::new().expect("create message");
    serde_cbor::to_writer(&mut msg, &protocol::Response::Error { message: format!("{}", e) }).expect("serialize error message");
    msg
}

struct Server {
    buffers: HashMap<protocol::BufferId, Buffer>,
    next_buffer_id: protocol::BufferId
}

impl Server {
    fn new() -> Self {
        Server {
            buffers: HashMap::new(),
            next_buffer_id: protocol::BufferId(1)
        }
    }

    fn process_request(&mut self, msg: protocol::Request) -> Result<protocol::Response, ServerError> {
        use protocol::*;
        match msg {
            Request::NewBuffer => {
                let buf = Buffer::default();
                let id = self.next_buffer_id;
                self.next_buffer_id = protocol::BufferId(self.next_buffer_id.0 + 1);
                let (id, contents, next_action_id) = (id, buf.text.text(), buf.text.next_action_id);
                self.buffers.insert(id, buf);
                Ok(Response::Buffer {
                    id, contents, next_action_id
                })
            },
            Request::OpenBuffer { path } => {
                let (id, contents, next_action_id) = 
                    if let Some((id, buf)) = self.buffers.iter().find(|b| b.1.path.as_ref().map(|p| *p == path).unwrap_or(false)) {
                        (*id, buf.text.text(), buf.text.next_action_id)
                    }
                    else {
                        let buf = Buffer::from_file(&path).map_err(ServerError::IoError)?;
                        let id = self.next_buffer_id;
                        self.next_buffer_id = protocol::BufferId(self.next_buffer_id.0 + 1);
                        let res = (id, buf.text.text(), buf.text.next_action_id);
                        self.buffers.insert(id, buf);
                        res
                    };
                Ok(Response::Buffer {
                    id,
                    contents,
                    next_action_id
                })
            },
            /*Request::SyncBuffer { id, changes } => {
              let buf = {this.read().unwrap().buffers[&id].clone()};
              let buf = buf.write().unwrap();
              for change in changes {
              buf.text.enact_change(&change);
              }
              Ok(Response::Ack)
              },*/
            _ => Err(ServerError::UnknownMessage)
        }
    }

    fn callback(server: Arc<RwLock<Self>>, aio: &nng::Aio, cx: &nng::Context, res: nng::AioResult) {
        match res {
            nng::AioResult::Send(Ok(_)) => while let Err(e) = cx.recv(aio) { println!("error recieving message {}", e); },
            nng::AioResult::Recv(Ok(raw_msg)) => {
                let resp = serde_cbor::from_slice(raw_msg.as_slice()).map_err(ServerError::MessageSerdeError)
                    .and_then(|req| server.write().unwrap().process_request(req))
                    .unwrap_or_else(|e| protocol::Response::Error { message: format!("{}", e) });
                let mut msg = nng::Message::new().expect("create message");
                serde_cbor::to_writer(&mut msg, &resp).expect("serialize message");
                cx.send(aio, msg).unwrap();
            },
            nng::AioResult::Recv(Err(e)) => { println!("error on recv {}", e); cx.recv(aio); },
            _ => panic!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_new_buffer() {
        let mut srv = Server::new();
        let rsp = srv.process_request(protocol::Request::NewBuffer).expect("response");
        match rsp {
            protocol::Response::Buffer { id, contents, next_action_id } => {
                assert_eq!(id, protocol::BufferId(1));
                let defp = piece_table::PieceTable::default();
                assert_eq!(contents, defp.text());
                assert_eq!(next_action_id, defp.next_action_id);
            },
            protocol::Response::Error { message } => panic!("error occurred: {}", message),
            _ => panic!("unexpected response: {:?}", rsp)
        }
    }

    #[test]
    fn msg_open_buffer() {
        let mut srv = Server::new();
        let rsp = srv.process_request(protocol::Request::OpenBuffer {
            path: std::path::PathBuf::from("Cargo.toml")
        }).expect("response");
        match rsp {
            protocol::Response::Buffer { id, contents, next_action_id } => {
                assert_eq!(id, protocol::BufferId(1));
                assert_eq!(contents, std::fs::read_to_string("Cargo.toml").expect("read test file"));
                assert_eq!(next_action_id, 1);
            },
            protocol::Response::Error { message } => panic!("error occurred: {}", message),
            _ => panic!("unexpected response: {:?}", rsp)
        }
    }

    #[test]
    fn msg_sync_buffer() {
        let mut srv = Server::new();
        let (mut pt, id) = match srv.process_request(protocol::Request::NewBuffer).expect("create buffer") {
            protocol::Response::Buffer { id, contents, next_action_id } => {
                (piece_table::PieceTable::with_text_and_starting_action_id(contents.as_str(), next_action_id), id)
            },
            protocol::Response::Error { message } => panic!("error occurred: {}", message),
            rsp@_ => panic!("unexpected response: {:?}", rsp)
        };

        pt.insert_range("hello, world!", 0);

        let rsp = srv.process_request(protocol::Request::OpenBuffer {
            path: std::path::PathBuf::from("Cargo.toml")
        }).expect("response");
        match rsp {
            protocol::Response::Buffer { id, contents, next_action_id } => {
                assert_eq!(id, protocol::BufferId(1));
                assert_eq!(contents, std::fs::read_to_string("Cargo.toml").expect("read test file"));
                assert_eq!(next_action_id, 1);
            },
            protocol::Response::Error { message } => panic!("error occurred: {}", message),
            _ => panic!("unexpected response: {:?}", rsp)
        }
    }

}


fn main() -> Result<(), ServerError> {
    let server_address = std::env::args().skip(1).next().expect("require nng url to listen on");

    let socket = nng::Socket::new(nng::Protocol::Rep0).map_err(ServerError::TransportError)?;

    //let pool = threadpool::ThreadPool::new(8);
    let mut server = Arc::new(RwLock::new(Server::new()));

    let ts = (0..8).map(|_| {
        let cx = nng::Context::new(&socket)?;
        let mcx = cx.clone();
        let server = server.clone();
        let aio = nng::Aio::new(move |aio, res| Server::callback(server.clone(), &aio, &cx, res))?;
        Ok((aio, mcx))

    }).collect::<Vec<nng::Result<_>>>();

    socket.listen(&server_address).map_err(ServerError::TransportError)?;

    for w in ts.iter() {
        match w {
            Ok((aio, cx)) => cx.recv(aio).map_err(ServerError::TransportError)?,
            Err(e) => println!("error starting worker thread {}", e)
        }
    }

    std::thread::park();

    /*loop {
        //let msg: protocol::Request = serde_cbor::from_slice(
        //    socket.recv().map_err(Error::from_other)?.as_slice()).map_err(Error::from_other)?;
        match reply_rx.try_recv() {
            Ok(Ok(raw_msg)) => socket.send(raw_msg).map_err(|(m,e)| Error::from_other(e))?,
            Ok(Err(e)) => socket.send(make_error_message(e)).map_err(|(m,e)| Error::from_other(e))?,
            Err(std::sync::mpsc::TryRecvError::Empty) => {},
            Err(e) => return Err(Error::from_other(e))
        };
        match socket.try_recv() {
            Ok(raw_msg) => {
                let reply_tx = reply_tx.clone();
                let server = server.clone();
                pool.execute(move|| Server::process_request(server, reply_tx, raw_msg));
            },
            Err(nng::Error::TryAgain) => {},
            Err(e) => return Err(Error::from_other(e))
        }
    }*/

    Ok(())
}
