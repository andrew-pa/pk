
use pk_common::protocol;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
enum ServerError { 
    MessageSerdeError(serde_cbor::Error),
    TransportError(nng::Error),
    IoError(std::io::Error),
    InternalError,
    BadFileId(protocol::FileId),
    UnknownMessage
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MessageSerdeError(e) => write!(f, "error serializing/deserializing message: {}", e),
            Self::TransportError(e) => write!(f, "error in transport: {}", e),
            Self::IoError(e) => write!(f, "io error: {}", e),
            Self::BadFileId(id) => write!(f, "unrecongized file id: {:?}", id),
            Self::UnknownMessage => write!(f, "unrecongized message recieved"),
            Self::InternalError => write!(f, "internal error"),
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

use std::path::PathBuf;

#[derive(Default)]
struct File {
    path: Option<PathBuf>,
    contents: String,
    current_version: usize
}

impl File {
    fn from_path<P: AsRef<std::path::Path>>(p: P) -> Result<File, ServerError> {
        Ok(File {
            path: Some({ let mut pa = PathBuf::new(); pa.push(&p); pa }),
            contents: std::fs::read_to_string(p).map_err(ServerError::IoError)?,
            current_version: 0
        })
    }

    fn write_to_disk(&self) -> Result<(), ServerError> {
        if let Some(path) = self.path.as_ref() {
            println!("writing {} v{} to disk", path.to_str().unwrap_or(""), self.current_version);
            std::fs::write(path, &self.contents).map_err(ServerError::IoError)?;
        }
        Ok(())
    }
}

struct Server {
    open_files: HashMap<protocol::FileId, File>,
    next_file_id: protocol::FileId
}

impl Server {
    fn new() -> Self {
        Server {
            open_files: HashMap::new(),
            next_file_id: protocol::FileId(1)
        }
    }

    fn process_request(&mut self, msg: protocol::Request) -> Result<protocol::Response, ServerError> {
        println!("request = {:?}", msg);
        use protocol::*;
        match msg {
            Request::NewFile { path } => {
                let buf = File::default();
                let id = self.next_file_id;
                self.next_file_id = protocol::FileId(self.next_file_id.0 + 1);
                let (id, contents, version) = (id, buf.contents.clone(), buf.current_version);
                self.open_files.insert(id, buf);
                Ok(Response::FileInfo {
                    id, contents, version
                })
            },
            Request::OpenFile { path } => {
                let (id, contents, version) = 
                    if let Some((id, buf)) = self.open_files.iter().find(|b| b.1.path.as_ref().map(|p| *p == path).unwrap_or(false)) {
                        (*id, buf.contents.clone(), buf.current_version)
                    }
                    else {
                        let buf = File::from_path(&path)?;
                        let id = self.next_file_id;
                        self.next_file_id = protocol::FileId(self.next_file_id.0 + 1);
                        let res = (id, buf.contents.clone(), buf.current_version);
                        self.open_files.insert(id, buf);
                        res
                    };
                Ok(Response::FileInfo {
                    id,
                    contents,
                    version
                })
            },
            Request::SyncFile { id, new_text, version } => {
                let file = self.open_files.get_mut(&id).ok_or_else(|| ServerError::BadFileId(id))?;
                if file.current_version >= version {
                    Ok(Response::VersionConflict {
                        id,
                        client_version_recieved: version,
                        server_version: file.current_version,
                        server_text: file.contents.clone()
                    })
                } else {
                    file.current_version = version;
                    file.contents = new_text;
                    Ok(Response::Ack)
                }
            },
            Request::CloseFile(id) => {
                self.open_files.remove(&id).ok_or_else(|| ServerError::BadFileId(id))?;
                Ok(Response::Ack)
            }
            _ => Err(ServerError::UnknownMessage)
        }
    }

    fn callback(server: Arc<RwLock<Self>>, aio: &nng::Aio, cx: &nng::Context, res: nng::AioResult) {
        match res {
            nng::AioResult::Send(Ok(_)) => while let Err(e) = cx.recv(aio) { println!("error recieving message {}", e); },
            nng::AioResult::Recv(Ok(raw_msg)) => {
                let resp = serde_cbor::from_slice(raw_msg.as_slice()).map_err(ServerError::MessageSerdeError)
                    .map(|req: protocol::MsgRequest| protocol::MsgResponse {
                        req_id: req.msg_id,
                        msg: server.write().unwrap()
                                .process_request(req.msg)
                                .unwrap_or_else(|err| protocol::Response::Error{message: format!("{}", err)})
                    }).unwrap_or_else(|err| protocol::MsgResponse {
                        req_id: protocol::MessageId(0),
                        msg: protocol::Response::Error { message: format!("error decoding request {}", err) }
                    });
                let mut msg = nng::Message::new().expect("create message");
                serde_cbor::to_writer(&mut msg, &resp).expect("serialize message");
                cx.send(aio, msg).unwrap();
            },
            nng::AioResult::Recv(Err(e)) => { println!("error on recv {}", e); cx.recv(aio).unwrap(); },
            _ => panic!()
        }
    }
}

struct AutosaveWorker {
    server: Arc<RwLock<Server>>,
    disk_versions: HashMap<protocol::FileId, usize>
}

impl AutosaveWorker {
    fn new(server: Arc<RwLock<Server>>) -> AutosaveWorker {
        AutosaveWorker {
            server, disk_versions: HashMap::new()
        }
    }

    fn run(&mut self) {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            let srv = self.server.read().unwrap();
            for (file_id, file) in srv.open_files.iter() {
                if let Some(last_disk_version) = self.disk_versions.insert(*file_id, file.current_version) {
                    if last_disk_version < file.current_version {
                        file.write_to_disk().unwrap();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

//     #[test]
//     fn msg_new_buffer() {
//         let mut srv = Server::new();
//         let rsp = srv.process_request(protocol::Request::NewBuffer).expect("response");
//         match rsp {
//             protocol::Response::Buffer { id, contents, next_action_id } => {
//                 assert_eq!(id, protocol::BufferId(1));
//                 let defp = piece_table::PieceTable::default();
//                 assert_eq!(contents, defp.text());
//                 assert_eq!(next_action_id, defp.next_action_id);
//             },
//             protocol::Response::Error { message } => panic!("error occurred: {}", message),
//             _ => panic!("unexpected response: {:?}", rsp)
//         }
//     }

//     #[test]
//     fn msg_open_buffer() {
//         let mut srv = Server::new();
//         let rsp = srv.process_request(protocol::Request::OpenBuffer {
//             path: std::path::PathBuf::from("Cargo.toml")
//         }).expect("response");
//         match rsp {
//             protocol::Response::Buffer { id, contents, next_action_id } => {
//                 assert_eq!(id, protocol::BufferId(1));
//                 assert_eq!(contents, std::fs::read_to_string("Cargo.toml").expect("read test file"));
//                 assert_eq!(next_action_id, 1);
//             },
//             protocol::Response::Error { message } => panic!("error occurred: {}", message),
//             _ => panic!("unexpected response: {:?}", rsp)
//         }
//     }

//     #[test]
//     fn msg_sync_buffer() {
//         let mut srv = Server::new();
//         let (mut pt, id) = match srv.process_request(protocol::Request::NewBuffer).expect("create buffer") {
//             protocol::Response::Buffer { id, contents, next_action_id } => {
//                 (piece_table::PieceTable::with_text_and_starting_action_id(contents.as_str(), next_action_id), id)
//             },
//             protocol::Response::Error { message } => panic!("error occurred: {}", message),
//             rsp@_ => panic!("unexpected response: {:?}", rsp)
//         };

//         pt.insert_range("hello, world!", 0);

//         match srv.process_request(protocol::Request::SyncBuffer { id, changes: pt.get_changes_from(0) }).expect("sync changes") {
//             protocol::Response::Ack => {
//                 assert_eq!(srv.buffers[&id].text.text(), pt.text());
//             },
//             protocol::Response::Error { message } => panic!("error occurred: {}", message),
//             rsp@_ => panic!("unexpected response: {:?}", rsp)
//         };
//     }

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

    let mut autosave_worker = AutosaveWorker::new(server.clone());
    std::thread::spawn(move || {
        autosave_worker.run();
    });

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
