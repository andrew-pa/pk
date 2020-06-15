use std::sync::{Arc,RwLock};
use std::collections::HashMap;
use futures::prelude::*;
use pk_common::*;
use crate::server::Server;
use pk_common::piece_table::PieceTable;
use crate::buffer::Buffer;

pub struct EditorState {
    pub buffers: Vec<Buffer>,
    pub current_buffer: usize,
    pub registers: HashMap<char, String>,
    pub command_line: Option<(usize, PieceTable)>,
    pub thread_pool: futures::executor::ThreadPool,
    pub servers: HashMap<String, Server>,
    pub force_redraw: bool
}

impl Default for EditorState {
    fn default() -> EditorState {
        EditorState {
            buffers: Vec::new(),
            current_buffer: 0,
            registers: HashMap::new(),
            command_line: None,
            thread_pool: futures::executor::ThreadPool::new().unwrap(),
            servers: HashMap::new(),
            force_redraw: false
        }
    }
}

pub type PEditorState = Arc<RwLock<EditorState>>;

impl EditorState {
    pub fn connect_to_server(&mut self, name: String, url: &str) -> Result<(), Error> {
        self.servers.insert(name, Server::init(url)?);
        Ok(())
    }

    pub fn make_request_async<F>(state: PEditorState, server_name: String, request: protocol::Request, f: F)
        -> Result<(), Error>
        where F: FnOnce(PEditorState, protocol::Response) + Send + Sync + 'static
    {
        let tp = {state.read().unwrap().thread_pool.clone()};
        let req_fut = {
            state.write().unwrap().servers.get_mut(&server_name)
                .ok_or(Error::InvalidCommand(String::from("server name ") + &server_name + " is unknown"))?
                .request(request)
        };
        let ess = state.clone();
        tp.spawn_ok(req_fut.then(move |resp: protocol::Response| async move
        {
            match resp {
                protocol::Response::Error { message } => {
                    ess.write().unwrap().process_error(message);
                },
                _ => f(ess, resp)
            }
        }));
        Ok(())
    }

    pub fn sync_buffer(state: PEditorState, buffer_index: usize) -> Result<(), Error> {
        let (server_name, id, new_text, version) = {
            let state = state.read().unwrap();
            let b = &state.buffers[buffer_index];
            (b.server_name.clone(), b.file_id, b.text.text(), b.version+1)
        };
        EditorState::make_request_async(state, server_name,
            protocol::Request::SyncFile { id, new_text, version },
            move |ess, resp| {
                match resp {
                    protocol::Response::Ack => {
                        let mut state = ess.write().unwrap();
                        state.buffers[buffer_index].version = version;
                    },
                    protocol::Response::VersionConflict { id, client_version_recieved,
                        server_version, server_text } =>
                    {
                        // TODO: probably need to show a nice little dialog, ask the user what they
                        // want to do about the conflict. this becomes a tricky situation since
                        // there's no reason to become Git, but it is nice to able to handle this
                        // situation in a nice way
                        todo!("version conflict!");
                    }
                    _ => panic!() 
                }
            }
        )?;
        Ok(())
    }
    
    pub fn process_error(&mut self, message: String) {
        println!("server error {}", message);
    }
}

pub struct AutosyncWorker {
    state: PEditorState,
    last_synced_action_ids: HashMap<String, HashMap<protocol::FileId, usize>> 
}

impl AutosyncWorker {
    pub fn new(state: PEditorState) -> AutosyncWorker {
        AutosyncWorker { state, last_synced_action_ids: HashMap::new() }
    }

    pub fn run(&mut self) {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            // should this function directly manipulate the futures? 
            // it would be possible to join all the request futures together and then poll them
            // with only one task, which would be more efficent.
            let mut need_sync = Vec::new();
            {
            let state = self.state.read().unwrap();
            for (i,b) in state.buffers.iter().enumerate() {
                if let Some(last_synced_action_id) = self.last_synced_action_ids
                    .entry(b.server_name.clone())
                        .or_insert_with(HashMap::new)
                    .insert(b.file_id, b.text.most_recent_action_id())
                {
                    if last_synced_action_id < b.text.most_recent_action_id() {
                        need_sync.push(i);
                    }
                }
            }
            }
            println!("autosync {:?}", need_sync);
            for i in need_sync {
                EditorState::sync_buffer(self.state.clone(), i).unwrap();
            }
        }
    }
}



