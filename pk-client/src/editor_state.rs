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
    
    pub fn process_error(&mut self, message: String) {
        println!("server error {}", message);
    }
}



