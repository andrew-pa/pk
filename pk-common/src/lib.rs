

use std::error::Error as ErrorTrait;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction { Forward, Backward }

pub mod piece_table;
use crate::piece_table::PieceTable;
pub mod command;

#[derive(Debug)]
pub enum Error {
    IncompleteCommand,
    InvalidCommand(String),
    UnknownCommand(String),
    Other(Box<dyn ErrorTrait + 'static>)
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IncompleteCommand => write!(f, "incomplete command"),
            Error::InvalidCommand(cmd) => write!(f, "invalid command: {}", cmd),
            Error::UnknownCommand(cmd) => write!(f, "unknown command: {}", cmd),
            Error::Other(e) => e.fmt(f)
        }
    }
}

impl ErrorTrait for Error {
    fn source(&self)  -> Option<&(dyn ErrorTrait + 'static)> {
        match self {
            Error::Other(e) => Some(e.as_ref()),
            _ => None
        }
    }
}

impl Error {
    pub fn from_other<E: ErrorTrait + 'static>(e: E) -> Self {
        Error::Other(Box::new(e))
    }
}

pub struct Buffer {
    pub text: PieceTable,
    pub path: Option<std::path::PathBuf>,
    pub cursor_index: usize
}

impl Buffer {
    pub fn with_text(s: &str) -> Buffer {
        Buffer {
            text: PieceTable::with_text(s),
            path: None,
            cursor_index: 0
        }
    }

    pub fn from_file(path: &std::path::Path) -> Result<Buffer, std::io::Error> {
        Ok(Buffer {
            text: PieceTable::with_text(&std::fs::read_to_string(path)?),
            path: Some(path.into()),
            cursor_index: 0,
        })
    }

    pub fn next_line_index(&self, at: usize) -> usize {
        self.text.index_of('\n', at).map(|i| i+1)
            .unwrap_or(0)
    }

    pub fn current_start_of_line(&self, at: usize) -> usize {
        self.text.last_index_of('\n', at).map(|i| i+1)
            .unwrap_or(0)
    }

    pub fn current_column(&self) -> usize {
        self.cursor_index - self.current_start_of_line(self.cursor_index)
    }

    pub fn last_line_index(&self, at: usize) -> usize {
        self.text.last_index_of('\n', at)
            .and_then(|eoll| self.text.last_index_of('\n', eoll)).map(|i| i+1)
            .unwrap_or(0)
    }


}

impl Default for Buffer {
    fn default() -> Buffer {
        Buffer {
            text: PieceTable::default(),
            path: None,
            cursor_index: 0
        }
    }
}


pub mod protocol {
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug)]
    pub struct BufferId(pub usize);

    #[derive(Serialize, Deserialize, Debug)]
    pub enum Request {
        /* buffers */
        NewBuffer,
        OpenBuffer { path: std::path::PathBuf },
        SyncBuffer { id: BufferId, changes: Vec<super::piece_table::Action> },
        ReloadBuffer(BufferId),
        CloseBuffer(BufferId)
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub enum Response {
        Ack,
        Error { message: String },
        Buffer { id: BufferId, contents: String, next_action_id: usize },
    }
}