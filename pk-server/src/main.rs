
use pk_common::protocol;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::path::{PathBuf, Path};

#[derive(Debug)]
enum ServerError { 
    MessageSerdeError(serde_cbor::Error),
    TransportError(nng::Error),
    IoError(std::io::Error),
    InternalError,
    BadFileId(protocol::FileId),
    UnknownMessage
}

impl From<serde_cbor::Error> for ServerError {
    fn from(e: serde_cbor::Error) -> Self {
        Self::MessageSerdeError(e)
    }
}

impl From<nng::Error> for ServerError {
    fn from(e: nng::Error) -> Self {
        Self::TransportError(e)
    }
}

impl From<std::io::Error> for ServerError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
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

mod filetype_table {
    use serde::Deserialize;
    use std::path::Path;

    #[derive(Deserialize, Debug)]
    pub struct FileType {
        pub name: String,
        pub ext: Vec<String>
    }

    #[derive(Deserialize, Debug)]
    pub struct FileTypeTable {
        filetype: Vec<FileType>
    }

    impl FileTypeTable {
        fn analyze(&self, path: impl AsRef<Path>) -> super::protocol::FileType {
        }
    }
}

use filetype_table::FileTypeTable;


#[derive(Default)]
struct File {
    path: Option<PathBuf>,
    contents: String,
    current_version: usize,
    format: protocol::TextFormat
}

impl File {
    fn analyze_file_for_type(filetype_table: &FileTypeTable, path: impl AsRef<Path>, cnt: impl AsRef<str>) -> protocol::FileType {
        protocol::FileType::from("text")
    }

    fn from_path<P: AsRef<Path>>(p: P, filetype_table: &FileTypeTable) -> Result<File, ServerError> {
        let path = Some({ let mut pa = PathBuf::new(); pa.push(&p); pa });
        let mut contents = match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(ServerError::IoError(e))
        };
        let fmt = protocol::TextFormat::from_analysis(&contents);
        if fmt.line_ending == protocol::LineEnding::CRLF {
            contents = contents.replace("\r\n", "\n");
        }
        Ok(File {
            format: fmt,
            path, contents,
            current_version: 0
        })
    }

    fn write_to_disk(&self) -> Result<(), ServerError> {
        if let Some(path) = self.path.as_ref() {
            println!("writing {} v{} to disk", path.to_str().unwrap_or(""), self.current_version);
            if self.format.line_ending == protocol::LineEnding::CRLF { 
                std::fs::write(path, self.contents.replace("\n", "\r\n"))?;
            } else {
                std::fs::write(path, &self.contents)?;
            }
        }
        Ok(())
    }
}

struct Server {
    open_files: HashMap<protocol::FileId, File>,
    next_file_id: protocol::FileId,
    filetype_table: FileTypeTable
}

impl Server {
    fn new(filetype_table: FileTypeTable) -> Self {
        Server {
            open_files: HashMap::new(),
            next_file_id: protocol::FileId(1),
            filetype_table
        }
    }

    fn process_request(&mut self, msg: protocol::Request) -> Result<protocol::Response, ServerError> {
        println!("request = {:?}", msg);
        use protocol::*;
        match msg {
            Request::OpenFile { path } => {
                let (id, contents, version, format) = 
                    if let Some((id, buf)) = self.open_files.iter().find(|b| b.1.path.as_ref().map(|p| *p == path).unwrap_or(false)) {
                        (*id, buf.contents.clone(), buf.current_version, buf.format.clone())
                    }
                    else {
                        let buf = File::from_path(&path, &self.filetype_table)?;
                        let id = self.next_file_id;
                        self.next_file_id = protocol::FileId(self.next_file_id.0 + 1);
                        let res = (id, buf.contents.clone(), buf.current_version, buf.format.clone());
                        self.open_files.insert(id, buf);
                        res
                    };
                Ok(Response::FileInfo {
                    id,
                    contents,
                    version,
                    format
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
                self.open_files.remove(&id)
                    .ok_or_else(|| ServerError::BadFileId(id))?
                    .write_to_disk()?;
                Ok(Response::Ack)
            }
            _ => Err(ServerError::UnknownMessage)
        }
    }

    fn callback(server: Arc<RwLock<Self>>, aio: &nng::Aio, cx: &nng::Context, res: nng::AioResult) {
        match res {
            nng::AioResult::Send(Ok(_)) => while let Err(e) = cx.recv(aio) { println!("error recieving message {}", e); },
            nng::AioResult::Recv(Ok(raw_msg)) => {
                let resp = serde_cbor::from_slice(raw_msg.as_slice())
                    .map(|req: protocol::MsgRequest| protocol::MsgResponse {
                        req_id: req.msg_id,
                        msg: server.write().unwrap()
                                .process_request(req.msg)
                                .unwrap_or_else(|err| protocol::Response::Error{message: format!("{}", err)})
                    }).unwrap_or_else(|err| protocol::MsgResponse {
                        req_id: protocol::MessageId(0),
                        msg: protocol::Response::Error { message: format!("error decoding request {}", err) }
                    });
                // println!("response = {:?}", resp);
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
                let disk_version = self.disk_versions.entry(*file_id).or_insert(0);
                if *disk_version < file.current_version {
                    println!("save v{} < v{} - {:?}", *disk_version,
                             file.current_version, file.path.as_ref());
                    match file.write_to_disk() {
                        Ok(()) => *disk_version = file.current_version,
                        Err(e) => {
                            println!("error syncing {} to disk: {}",
                                     file.path.as_ref().and_then(|p| p.to_str()).unwrap_or(""), e);
                        }
                    }
                }
            }
        }
    }
}

fn main() -> Result<(), ServerError> {
    let server_address = std::env::args().skip(1).next().expect("require nng url to listen on");

    let socket = nng::Socket::new(nng::Protocol::Rep0)?;

    //let pool = threadpool::ThreadPool::new(8);
    let filetype_table = toml::from_str(&std::fs::read_to_string("./filetypes.toml")?).expect("parse filetype table");
    println!("filetypes = {:?}", filetype_table);
    let server = Arc::new(RwLock::new(Server::new(filetype_table)));

    let ts = (0..8).map(|_| {
        let cx = nng::Context::new(&socket)?;
        let mcx = cx.clone();
        let server = server.clone();
        let aio = nng::Aio::new(move |aio, res| Server::callback(server.clone(), &aio, &cx, res))?;
        Ok((aio, mcx))

    }).collect::<Vec<nng::Result<_>>>();

    socket.listen(&server_address)?;

    for w in ts.iter() {
        match w {
            Ok((aio, cx)) => cx.recv(aio)?,
            Err(e) => println!("error starting worker thread {}", e)
        }
    }

    let mut autosave_worker = AutosaveWorker::new(server.clone());
    std::thread::spawn(move || {
        autosave_worker.run();
    });

    std::thread::park();

    Ok(())
}
