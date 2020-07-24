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
    pub highlights: Option<Vec<crate::piece_table_render::Highlight>>,
    pub last_highlighted_action_id: usize,
    pub current_query: Option<String>
}

impl Buffer {
    pub fn with_text(s: &str) -> Buffer {
        Buffer {
            text: PieceTable::with_text(s),
            version: 0, file_id: protocol::FileId(0), cursor_index: 0,
            server_name: "".into(),
            path: "".into(), currently_in_conflict: false, format: protocol::TextFormat::default(),
            highlights: None,
            last_highlighted_action_id: 0,
            current_query: None
        }
    }

    pub fn from_server(server_name: String, path: PathBuf, file_id: protocol::FileId, contents: String, version: usize, format: protocol::TextFormat) -> Buffer {
        Buffer {
            text: PieceTable::with_text(&contents),
            file_id, version, cursor_index: 0,
            server_name, path,
            currently_in_conflict: false, format,
            highlights: None,
            last_highlighted_action_id: 0,
            current_query: None
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
    
    pub fn set_query(&mut self, s: String) {
        self.current_query = Some(s);
    }
    
    pub fn next_query_index(&self, from: usize, direction: crate::Direction, mut wrap: bool) -> Option<usize> {
        if self.current_query.is_none() { return None; }
        let mut chrs = self.text.chars(from);
        let qury: Vec<char> = self.current_query.as_ref().unwrap().chars().collect();
        let mut qury_ix = 0;
        let mut chrs_ix = from;
        let mut qury_match = None;
        loop {
            //dbg!(&qury, qury_ix, chrs_ix, qury_match);
            match chrs.next() {
                Some(c) if qury[qury_ix] == c => {
                    if qury_ix == 0 {
                        qury_match = Some(chrs_ix);
                    }
                    qury_ix += 1;
                    if qury_ix == qury.len() {
                        return qury_match;
                    }
                },
                Some(_) => {
                    qury_ix = 0;
                    qury_match = None;
                },
                None => {
                    if wrap {
                        wrap = false;
                        chrs_ix = 0;
                        chrs = self.text.chars(from);
                    } else {
                        break;
                    }
                }
            }
            chrs_ix += 1;
        }
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn search_forwards() {
        let src = "abc def abc abl abc pqr abc vwx yz\n";
        let qry = "abc";
        let mut buf = Buffer::with_text(src);
        buf.set_query(qry.into());
        let mut ix = 0;
        loop {
            println!("cr = {}", &src[ix..].escape_debug());
            match (buf.next_query_index(ix, crate::Direction::Forward, false), &src[ix..].find(qry).map(|i| i + ix)) {
                (Some(test), Some(correct)) => {
                    println!("found instance at {}, {}", test, correct);
                    assert_eq!(test, *correct);
                    ix = test + qry.len();
                    if ix > src.len() { break; }
                },
                (Some(x), None) => { panic!("buffer found an instance at {} that was invalid", x); },
                (None, Some(x)) => { panic!("buffer missed an instance at {}", x); },
                (None, None) => break
            }
        }
    }
    
    #[test]
    fn search_backwards() {
        let src = "abc def abc abl abc pqr abc vwx yz\n";
        let qry = "abc";
        let mut buf = Buffer::with_text(src);
        buf.set_query(qry.into());
        let mut ix = src.len()-1;
        loop {
            println!("cr = {}", &src[..ix].escape_debug());
            match (buf.next_query_index(ix, crate::Direction::Backward, false), &src[..ix].rfind(qry)) {
                (Some(test), Some(correct)) => {
                    println!("found instance at {}, {}", test, correct);
                    assert_eq!(test, *correct);
                    ix = test.saturating_sub(1);
                    if ix == 0 { break; }
                },
                (Some(x), None) => { panic!("buffer found an instance at {} that was invalid", x); },
                (None, Some(x)) => { panic!("buffer missed an instance at {}", x); },
                (None, None) => break
            }
        }
    }
}

