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

impl EditorState {
    pub fn connect_to_server(&mut self, name: String, url: &str) -> Result<(), Error> {
        self.servers.insert(name, Server::init(url)?);
        Ok(())
    }
}

pub type PEditorState = Arc<RwLock<EditorState>>;


