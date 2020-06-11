use std::sync::{Arc,RwLock};
use std::collections::HashMap;
use futures::prelude::*;
use pk_common::*;
use crate::server::Server;
use pk_common::piece_table::PieceTable;

pub struct EditorState {
    pub buffers: Vec<Buffer>,
    pub current_buffer: usize,
    pub registers: HashMap<char, String>,
    pub command_line: Option<(usize, PieceTable)>,
    pub thread_pool: futures::executor::ThreadPool,
    pub servers: HashMap<String, Server>
}

impl Default for EditorState {
    fn default() -> EditorState {
        EditorState {
            buffers: vec![Buffer::from_file(&std::path::Path::new("pk-runic-client/src/main.rs")).unwrap()],
            current_buffer: 0,
            registers: HashMap::new(),
            command_line: None,
            thread_pool: futures::executor::ThreadPool::new().unwrap(),
            servers: HashMap::new() 
        }
    }
}

impl EditorState {
    pub fn connect_to_server(&mut self, name: String, url: &str) -> Result<(), Error> {
        self.servers.insert(name, Server::init(url)?);
        Ok(())
    }
}

pub type PEditorState = Arc<RwLock<EditorState>>;


