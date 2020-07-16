use pk_common::piece_table::*;
use pk_common::protocol;
use std::path::PathBuf;

pub struct Buffer {
    pub text: PieceTable,
    pub server_name: String,
    pub path: PathBuf,
    pub file_id: protocol::FileId,
    pub format: protocol::TextFormat,
    pub version: usize,
    pub currently_in_conflict: bool,
    pub cursor_index: usize,
    pub highlights: Option<Vec<crate::piece_table_render::Highlight>>
}

impl Buffer {
    pub fn with_text(s: &str) -> Buffer {
        Buffer {
            text: PieceTable::with_text(s),
            version: 0, file_id: protocol::FileId(0), cursor_index: 0,
            server_name: "".into(),
            path: "".into(), currently_in_conflict: false, format: protocol::TextFormat::default(),
            highlights: None
        }
    }

    pub fn from_server(server_name: String, path: PathBuf, file_id: protocol::FileId, contents: String, version: usize, format: protocol::TextFormat) -> Buffer {
        Buffer {
            text: PieceTable::with_text(&contents),
            file_id, version, cursor_index: 0,
            server_name, path,
            currently_in_conflict: false, format,
            highlights: None
        }
    }

    pub fn sense_indent_level(&self, at: usize, config: &crate::config::Config) -> usize {
        let mut i = self.current_start_of_line(at);
        let mut indent_level = 0;
        if config.softtab {
            let mut space_counter = 0;
            loop {
                match self.text.char_at(i) {
                    Some(' ') => {
                        space_counter += 1;
                        if space_counter == 4 {
                            space_counter = 0;
                            indent_level += 1;
                        }
                    },
                    Some('\t') => indent_level += 1,
                    Some(_) | None => break,
                }
                i += 1;
            }
        } else {
            loop {
                match self.text.char_at(i) {
                    Some('\t') => indent_level += 1,
                    Some(_) | None => break
                }
                i += 1;
            }
        }
        indent_level
    }

    pub fn indent_with_mutator(&mut self, ins: &mut crate::piece_table::TableMutator, count: usize, config: &crate::config::Config) -> usize {
        if count == 0 { return 0; }
        if config.softtab {
            for _ in 0..(count*config.tabstop) {
                ins.push_char(&mut self.text, ' ');
            }
            count * config.tabstop
        } else {
            for _ in 0..count {
                ins.push_char(&mut self.text, '\t');
            }
            count
        }
    }

    pub fn indent(&mut self, at: usize, count: usize, config: &crate::config::Config) -> usize {
        if count == 0 { return 0; }
        if config.softtab {
            let mut ins = self.text.insert_mutator(at);
            for _ in 0..(count*config.tabstop) {
                ins.push_char(&mut self.text, ' ');
            }
            ins.finish(&mut self.text);
            count * config.tabstop
        } else {
            for _ in 0..count {
                self.text.insert_range("\t", at);
            }
            count
        }
    }

    pub fn undent(&mut self, at: usize, count: usize, config: &crate::config::Config) {
        if count == 0 { return; }
        if config.softtab { 
            let mut spaces_left = count * config.tabstop;
            loop {
                match self.text.char_at(at) {
                    Some(c) if c.is_whitespace() => {
                        self.text.delete_range(at, at+1);
                        spaces_left -= 1;
                    }
                    Some('\t') => {
                        self.text.delete_range(at, at+1);
                        spaces_left -= config.tabstop;
                    }
                    Some(_) | None => break
                }
                if spaces_left == 0 { break; }
            }
        } else {
            for _ in 0..count {
                match self.text.char_at(at) {
                    Some('\t') => {
                        self.text.delete_range(at, at+1);
                    }
                    Some(_) | None => break
                }
            }
        }
    }
    
    //prev line\nthis is a line\nnext line
    //^LLL       ^CSoL           ^NL

    pub fn next_line_index(&self, at: usize) -> usize {
        self.text.index_of('\n', at).map(|i| i+1)
            .unwrap_or(self.text.len())
    }

    pub fn current_start_of_line(&self, at: usize) -> usize {
        self.text.last_index_of('\n', at).map(|i| i+1)
            .unwrap_or(0)
    }

    pub fn column_for_index(&self, index: usize) -> usize {
        index - self.current_start_of_line(index)
    }

    pub fn line_for_index(&self, index: usize) -> usize {
        let mut ln = 0;
        let mut ix = 0;
        loop {
           if let Some(nix) = self.text.index_of('\n', ix) {
               if index >= ix && index <= nix { return ln; }
               ln += 1;
               ix = nix+1;
           } else {
               break;
           }
        }
        ln
    }

    pub fn last_line_index(&self, at: usize) -> usize {
        self.text.last_index_of('\n', at)
            .and_then(|eoll| self.text.last_index_of('\n', eoll)).map(|i| i+1)
            .unwrap_or(0)
    }
}



