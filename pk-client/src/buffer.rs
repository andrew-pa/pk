use pk_common::piece_table::*;
use pk_common::protocol;
use std::path::PathBuf;

pub struct Buffer {
    pub text: PieceTable,
    pub server_name: String,
    pub path: PathBuf,
    pub file_id: protocol::FileId,
    pub version: usize,
    pub cursor_index: usize,
    pub currently_in_conflict: bool
}

impl Buffer {
    pub fn with_text(s: &str) -> Buffer {
        Buffer {
            text: PieceTable::with_text(s),
            version: 0, file_id: protocol::FileId(0),
            cursor_index: 0, server_name: "".into(),
            path: "".into(), currently_in_conflict: false
        }
    }

    pub fn from_server(server_name: String, path: PathBuf, file_id: protocol::FileId, contents: String, version: usize) -> Buffer {
        Buffer {
            text: PieceTable::with_text(&contents),
            file_id, version,
            cursor_index: 0, server_name, path,
            currently_in_conflict: false
        }
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



