#![allow(dead_code)]
#![allow(unused_variables)]
use serde::{Serialize, Deserialize};
use super::Direction;

#[derive(Copy,Clone,Debug, Serialize, Deserialize)]
pub struct Piece {
    pub source: usize,
    pub start: usize, pub length: usize
}

impl Piece {
    /// `at` is relative to the beginning of the piece
    fn split(self, at: usize) -> (Piece, Piece) {
        assert!(at <= self.length);
        (Piece { source: self.source, start: self.start, length: at },
         Piece { source: self.source, start: self.start+at, length: self.length-at })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Change {
    Insert {
        piece_index: usize, new: Piece
    },
    Modify {
        piece_index: usize,
        old: Piece, new: Piece
    },
    Delete {
        piece_index: usize,
        old: Piece
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Action {
    changes: Vec<Change>,
    id: usize
}

impl Action {
    fn new(pt: &mut PieceTable) -> Action {
        let id = pt.next_action_id;
        pt.next_action_id += 1;
        Action {
            changes: Vec::new(), id
        }
    }

    fn push(&mut self, c: Change) {
        self.changes.push(c);
    }

    fn iter(&self) -> impl DoubleEndedIterator<Item=&Change> {
        self.changes.iter()
    }
}


#[derive(Debug)]
pub struct PieceTable {
    pub sources: Vec<String>,
    pub pieces: Vec<Piece>,
    pub history: Vec<Action>,
    pub next_action_id: usize
}

impl Default for PieceTable {
    fn default() -> PieceTable {
        PieceTable::with_text("Hello, world!\n\nthis is\na test\n\nline!\n")
    }
}

pub struct TableMutator {
    piece_ix: usize
}
impl TableMutator {
    pub fn push_char(&mut self, pt: &mut PieceTable, c: char) {
        pt.pieces[self.piece_ix].length += 1;
        let si = pt.pieces[self.piece_ix].source;
        pt.sources[si].push(c);
    }

    pub fn pop_char(&mut self, pt: &mut PieceTable) -> bool {
        if pt.pieces[self.piece_ix].length == 0 {
            return true;
        }
        pt.pieces[self.piece_ix].length -= 1;
        let si = pt.pieces[self.piece_ix].source;
        pt.sources[si].pop();
        false
    }
}

pub struct TableChars<'table> {
    table: &'table PieceTable,
    current_piece: usize,
    current_index: usize,
    hit_beginning: bool,
}

impl<'t> Iterator for TableChars<'t> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        if self.current_piece >= self.table.pieces.len() { return None; }
        let curp = &self.table.pieces[self.current_piece];
        let ch = self.table.sources[curp.source].chars().nth(curp.start + self.current_index);
        self.current_index += 1;
        if self.current_index >= curp.length {
            self.current_piece += 1;
            self.current_index = 0;
        }
        ch
    }
}

impl<'t> DoubleEndedIterator for TableChars<'t> {
    fn next_back(&mut self) -> Option<char> {
        if self.hit_beginning { return None; }
        let curp = &self.table.pieces[self.current_piece];
        let ch = self.table.sources[curp.source].chars().nth(curp.start + self.current_index);
        if self.current_index == 0 {
            if self.current_piece == 0 {
                self.hit_beginning = true;
            } else {
                self.current_piece -= 1;
                self.current_index = self.table.pieces[self.current_piece].length-1;
            }
        } else {
            self.current_index -= 1;
        }
        ch
    }
}


impl<'table> PieceTable {
    pub fn with_text(s: &str) -> PieceTable {
        PieceTable::with_text_and_starting_action_id(s, 1)
    }

    pub fn with_text_and_starting_action_id(s: &str, start_aid: usize) -> PieceTable {
        PieceTable {
            sources: vec![s.to_string()],
            pieces: vec![ Piece { source: 0, start: 0, length: s.len() } ],
            history: Vec::new(), next_action_id: start_aid 
        }
    }

    fn enact_change(&mut self, change: &Change) {
        match *change {
            Change::Insert { piece_index, new } => {
                self.pieces.insert(piece_index, new);
            },
            Change::Modify { piece_index, new, .. } => {
                self.pieces[piece_index] = new;
            },
            Change::Delete { piece_index, .. } => {
                self.pieces.remove(piece_index);
            },
        }
    }

    fn reverse_change(&mut self, change: &Change) {
        match *change {
            Change::Insert { piece_index, .. } => {
                self.pieces.remove(piece_index);
            },
            Change::Modify { piece_index, old, .. } => {
                self.pieces[piece_index] = old;
            },
            Change::Delete { piece_index, old } => {
                self.pieces.insert(piece_index, old);
            },
        }
    }

    pub fn undo(&mut self) {
        if let Some(action) = self.history.pop() {
            for change in action.iter().rev() {
                self.reverse_change(&change);
            }
        }
    }

    pub fn insert_range(&mut self, s: &str, index: usize) {
        let mut ix = 0usize;
        let new_piece = Piece { source: self.sources.len(), start: 0, length: s.len() };
        self.sources.push(String::from(s));
        let mut action = Action::new(self);
        for (i,p) in self.pieces.iter().enumerate() {
            println!("{} {:?}", i, p);
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    self.pieces.insert(i, new_piece);
                    action.push(Change::Insert{piece_index: i, new: new_piece});
                } else if index == ix+p.length { // we're inserting at the end of this piece
                    self.pieces.insert(i+1, new_piece);
                    action.push(Change::Insert{piece_index: i+1, new: new_piece});
                } else { //insertion in the middle
                    let (left,right) = p.split(index-ix);
                    action.push(Change::Modify{piece_index: i, old: p.clone(), new: left.clone()});
                    action.push(Change::Insert{piece_index: i+1, new: new_piece});
                    action.push(Change::Insert{piece_index: i+2, new: right});
                    self.pieces[i] = left;
                    self.pieces.insert(i+1, new_piece);
                    self.pieces.insert(i+2, right);
                }
                break;
            }
            ix += p.length;
        }
        self.history.push(action);
    }

    pub fn insert_mutator(&mut self, index: usize) -> TableMutator {
        let mut ix = 0usize;
        let mut insertion_piece_index: Option<usize> = None;
        let mut action = Action::new(self);
        for (i,p) in self.pieces.iter().enumerate() {
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    let new = Piece { source: self.sources.len(), start: 0, length: 0 };
                    self.pieces.insert(i, new);
                    self.sources.push(String::new());
                    action.push(Change::Insert { piece_index: i, new });
                    insertion_piece_index = Some(i);
                } else if index == ix+p.length-1 { // we're inserting at the end of this piece
                    if self.sources[p.source].len() ==
                        p.start+p.length { // we're inserting at the current end of the piece in the source
                        insertion_piece_index = Some(i);
                    } else {
                        let new = Piece { source: self.sources.len(), start: 0, length: 0 }; 
                        self.pieces.insert(i+1, new);
                        action.push(Change::Insert { piece_index: i+1, new });
                        self.sources.push(String::new());
                        insertion_piece_index = Some(i+1);
                    }
                } else { //insertion in the middle
                    let (a,c) = p.split(index-ix);
                    let b = Piece { source: self.sources.len(), start: 0, length: 0 };
                    self.sources.push(String::new());
                    action.push(Change::Modify { piece_index: i, old: p.clone(), new: a.clone() });
                    action.push(Change::Insert { piece_index: i+1, new: b });
                    action.push(Change::Insert { piece_index: i+2, new: c });
                    self.pieces[i] = a;
                    self.pieces.insert(i+1, b);
                    self.pieces.insert(i+2, c);
                    insertion_piece_index = Some(i+1);
                }
                break;
            }
            ix += p.length;
        }
        self.history.push(action);
        let insertion_piece_index = insertion_piece_index.unwrap();
        TableMutator { piece_ix: insertion_piece_index }
    }

    pub fn delete_range(&mut self, start: usize, mut end: usize) {
        let mut start_piece: Option<(usize,usize)> = None;
        let mut end_piece:   Option<(usize,usize)> = None;
        let mut mid_pieces:  Vec<usize>            = Vec::new();
        let mut global_index                       = 0usize;
        let mut action = Action::new(self);

        for (i,p) in self.pieces.iter().enumerate() {
            if start < global_index && end >= global_index+p.length {
                mid_pieces.push(i);
            } else if start >= global_index && start <= global_index+p.length {
                if end >= global_index && end < global_index+p.length {
                    // this piece totally contains this range
                    if start-global_index == 0 {
                        if end-global_index == 0 {
                            let mut np = p.clone();
                            np.start += 1;
                            np.length -= 1;
                            if np.length == 0 {
                                action.push(Change::Delete { piece_index: i, old: self.pieces.remove(i) });
                            } else {
                                action.push(Change::Modify { piece_index: i, old: *p, new: np });
                                self.pieces[i] = np;
                            }
                        } else if end-global_index == p.length {
                            action.push(Change::Delete { piece_index: i, old: self.pieces.remove(i) });
                        }
                    } else {
                        let (left_keep, deleted_right) = p.split(start-global_index);
                        assert!(left_keep.length != 0, "{} {} {}, {}", start, end, global_index, p.length);
                        let (_deleted, right_keep) = deleted_right.split(end-(global_index+left_keep.length-1));
                        action.push(Change::Modify { piece_index: i, old: p.clone(), new: left_keep });
                        action.push(Change::Insert { piece_index: i+1, new: right_keep });
                        self.pieces[i] = left_keep;
                        self.pieces.insert(i+1, right_keep);
                    }
                    self.history.push(action);
                    return;
                } else {
                    start_piece = Some((i, start-global_index));
                }
            } else if end >= global_index && end < global_index+p.length {
                end_piece = Some((i, end-global_index));
            }
            global_index += p.length;
        }

        println!("start: {:?}\nend: {:?}\nmid: {:?}", start_piece, end_piece, mid_pieces);

        let (start_piece, start_cut) = start_piece.unwrap();
        let (end_piece,   end_cut)   = end_piece.unwrap();

        let new_start = self.pieces[start_piece].split(start_cut).0;
        let new_end   = self.pieces[end_piece].split(end_cut).1;

        action.push(Change::Modify { piece_index: start_piece, old: self.pieces[start_piece], new: new_start });
        action.push(Change::Modify { piece_index: end_piece, old: self.pieces[end_piece], new: new_end });

        self.pieces[start_piece] = new_start;
        self.pieces[end_piece]   = new_end;

        for i in mid_pieces {
            action.push(Change::Delete { piece_index: i,  old: self.pieces.remove(i) });
        }

        self.history.push(action);
    }

    pub fn copy_range(&mut self, start: usize, end: usize) -> String {
        let mut buf = String::new();
        let mut global_index = 0usize;
        for p in self.pieces.iter() {
            println!("{}", buf);
            if start < global_index && end >= global_index+p.length {
                buf.push_str(&self.sources[p.source][p.start..(p.start + p.length)]); 
            } else if start >= global_index && start < global_index+p.length {
                if end > global_index && end <= global_index+p.length {
                    buf.push_str(&self.sources[p.source][p.start+(start-global_index)..(p.start + (end-global_index+1))]);
                    break;
                } else {
                    buf.push_str(&self.sources[p.source][(p.start + start-global_index)..(p.start + p.length)]);
                }
            } else if end >= global_index && end < global_index+p.length {
                buf.push_str(&self.sources[p.source][p.start .. (p.start + end-global_index+1)]);
            }
            global_index += p.length;
        }
        buf
    }

    pub fn index_of(&self, sc: char, start: usize) -> Option<usize> {
        self.index_of_pred(|c| c == sc, start)
    }

    pub fn index_of_pred<P: Fn(char)->bool>(&self, pred: P, start: usize) -> Option<usize> {
        let mut global_index = 0usize;
        for p in self.pieces.iter() {
            let search_start_in_piece = if global_index+p.length <= start { global_index += p.length; continue; }
            else if start >= global_index && start < global_index+p.length {
                // starting inside this piece
                start - global_index
            } else {
                0
            };
            if let Some(result_local_index)
                    = self.sources[p.source][(p.start+search_start_in_piece)..(p.start+p.length)].find(&pred) {
                return Some(search_start_in_piece + result_local_index + global_index);
            }
            global_index += p.length;
        }
        None
    }

    // |----a----|-----b----|----c----|
    //              ^                 $

    pub fn last_index_of(&self, c: char, start: usize) -> Option<usize> {
        self.last_index_of_pred(|sc| c == sc, start)
    }

    pub fn last_index_of_pred<P: Fn(char)->bool>(&self, pred: P, start: usize) -> Option<usize> {
        let mut global_index_end = self.pieces.iter().fold(0, |a,p| a+p.length);
        //println!("~~{}",start);
        for p in self.pieces.iter().rev() {
            //println!("{:?}, gie{} '{}'", p, global_index_end, &self.sources[p.source][p.start..p.start+p.length]);
            // this piece comes entirely after `start` and so it isn't included in the search
            if start <= global_index_end-p.length { /*println!("S");*/ global_index_end -= p.length; continue; }  
            // this piece has `start` between its bounds
            else if start > global_index_end-p.length && start <= global_index_end {
                let piece_local_start = start - (global_index_end-p.length);
                //println!("I {}", &self.sources[p.source][p.start..(p.start+piece_local_start)]);
                if let Some(result_local_index) = self.sources[p.source][p.start..(p.start+piece_local_start)].rfind(&pred) {
                    //println!("{} {}", result_local_index, global_index_end-p.length);
                    return Some(global_index_end-p.length + result_local_index);
                }
            }
            // this piece is totally contained by the range
            else {
                //print!("C");
                if let Some(result_local_index) = self.sources[p.source][p.start..p.start+p.length].rfind(&pred) {
                    //println!("c{} '{}' {}", result_local_index, &self.sources[p.source][p.start..p.start+p.length], global_index_end-p.length);
                    return Some(global_index_end-p.length + result_local_index);
                }
            }
            global_index_end -= p.length;
        }
        None
    }

    pub fn dir_index_of<P: Fn(char)->bool>(&self, pred: P, start: usize, dir: Direction) -> Option<usize> {
        match dir {
            Direction::Forward => self.index_of_pred(pred, start),
            Direction::Backward => self.last_index_of_pred(pred, start)
        }
    }

    pub fn char_at(&self, index: usize) -> Option<char> {
        let mut global_index = 0;
        for p in self.pieces.iter() {
            if index >= global_index && index < global_index+p.length { 
                return self.sources[p.source].chars().nth(p.start + index-global_index);
            }
            global_index += p.length;
        }
        None
    }

    pub fn chars(&self, index: usize) -> TableChars {
        let mut global_index = 0;
        for (pi, p) in self.pieces.iter().enumerate() {
            if index >= global_index && index <= global_index+p.length { 
                return TableChars {
                    table: self,
                    current_piece: pi,
                    current_index: index - global_index,
                    hit_beginning: false
                };
            }
            global_index += p.length;
        }
        panic!("tried to start char iterator out of bounds");
    }

    pub fn text(&self) -> String {
        let mut s = String::with_capacity(self.len());
        for piece in self.pieces.iter() {
           s.push_str(&self.sources[piece.source][piece.start..(piece.start+piece.length)]); 
        }
        s
    }

    pub fn len(&self) -> usize {
        self.pieces.iter().map(|p| p.length).sum()
    }
}



#[cfg(test)]
mod tests {
    use crate::piece_table::*;

    #[test]
    fn insert_cont() {
        let mut pt = PieceTable::with_text("hello");
        let mut m = pt.insert_mutator(2);
        m.push_char(&mut pt, 'A');
        m.push_char(&mut pt, 'B');
        assert_eq!(pt.text(), "heABllo");
        m.pop_char(&mut pt);
        m.push_char(&mut pt, 'C');
        assert_eq!(pt.text(), "heACllo");
        println!("{:#?}", pt);
    }
   
    #[test]
    fn insert_range() {
        let mut pt = PieceTable::with_text("hi");
        pt.insert_range("ABCD", 1);
        assert_eq!(pt.text(), "hABCDi");
        println!("{:#?}", pt);
    }

    #[test]
    fn delete_range_single_piece() {
        let mut pt = PieceTable::with_text("hello");
        pt.delete_range(1,3);
        assert_eq!(pt.text(), "ho");
        println!("{:#?}", pt);
    }
 
    #[test]
    fn delete_range_multiple_pieces() {
        let mut pt = PieceTable::with_text("hello");
        pt.insert_range("X", 3); //hel|X|lo
        println!("{:#?}", pt);
        pt.delete_range(1,4);
        println!("{:#?}", pt);
        assert_eq!(pt.text(), "hlo");
        println!("{:#?}", pt);
    }

    #[test]
    fn delete_range_single_char() {
        let mut pt = PieceTable::with_text("hello");
        pt.delete_range(2,2);
        assert_eq!(pt.text(), "helo");
        println!("{:#?} {}", pt, pt.text());
        pt.delete_range(2,2);
        assert_eq!(pt.text(), "heo");
        println!("{:#?} {}", pt, pt.text());
        pt.delete_range(1,1);
        println!("{:#?} {}", pt, pt.text());
        assert_eq!(pt.text(), "ho");
        println!("{:#?}", pt);
    }

    #[test]
    fn copy_range_single_piece() {
        let mut pt = PieceTable::with_text("hello");
        assert_eq!(pt.copy_range(1,3), "ell");
        println!("{:#?}", pt);
    }
 
    #[test]
    fn copy_range_multiple_pieces() {
        let mut pt = PieceTable::with_text("hello");
        pt.insert_range("X", 3); //hel|X|lo
        println!("{:#?}", pt);
        assert_eq!(pt.copy_range(1,4), "elXl");
        println!("{:#?}", pt);
    }

    #[test]
    fn undo_insert_cont() {
        let mut pt = PieceTable::with_text("hello");
        let mut m = pt.insert_mutator(2);
        m.push_char(&mut pt, 'A');
        m.push_char(&mut pt, 'B');
        assert_eq!(pt.text(), "heABllo");
        m.pop_char(&mut pt);
        m.push_char(&mut pt, 'C');
        assert_eq!(pt.text(), "heACllo");
        pt.undo();
        assert_eq!(pt.text(), "hello");
    }
   

    #[test]
    fn undo_insert_range_once() {
        let mut pt = PieceTable::with_text("hi");
        pt.insert_range("ABCD", 1);
        assert_eq!(pt.text(), "hABCDi");
        pt.undo();
        assert_eq!(pt.text(), "hi");
    }
    
    #[test]
    fn undo_insert_range_multiple() {
        let mut pt = PieceTable::with_text("hi");
        pt.insert_range("ABCD", 1);
        pt.insert_range("X", 2);
        assert_eq!(pt.text(), "hAXBCDi");
        pt.undo();
        assert_eq!(pt.text(), "hABCDi");
        pt.undo();
        assert_eq!(pt.text(), "hi");
    }

    #[test]
    fn undo_delete_range_single_piece() {
        let mut pt = PieceTable::with_text("hello");
        pt.delete_range(1,3);
        assert_eq!(pt.text(), "ho");
        pt.undo();
        assert_eq!(pt.text(), "hello");
    }
 
    #[test]
    fn undo_delete_range_multiple_pieces() {
        let mut pt = PieceTable::with_text("hello");
        pt.insert_range("X", 3); //hel|X|lo
        println!("{:#?}", pt);
        pt.delete_range(1,4);
        assert_eq!(pt.text(), "hlo");
        pt.undo();
        assert_eq!(pt.text(), "helXlo");
        pt.undo();
        assert_eq!(pt.text(), "hello");
    }

    #[test]
    fn index_of_simple() {
        let pt = PieceTable::with_text("he?lo?a");
        assert_eq!(pt.index_of('?', 0), Some(2));
        assert_eq!(pt.index_of('x', 0), None);
        assert_eq!(pt.index_of('?', 3), Some(5));
    }

    #[test]
    fn index_of_complex() {
        let mut pt = PieceTable::with_text("helo?a");
        pt.insert_range("?", 2);
        assert_eq!(pt.text(), "he?lo?a");
        assert_eq!(pt.index_of('?', 0), Some(2));
        assert_eq!(pt.index_of('x', 0), None);
        assert_eq!(pt.index_of('?', 3), Some(5));
    }

    #[test]
    fn last_index_of_simple() {
        let pt = PieceTable::with_text("he?lo?a");
        assert_eq!(pt.last_index_of('?', 3), Some(2));
        assert_eq!(pt.last_index_of('x', 6), None);
        assert_eq!(pt.last_index_of('?', 6), Some(5));
    }

    #[test]
    fn last_index_of_complex() {
        let mut pt = PieceTable::with_text("helo?a");
        pt.insert_range("?", 2);
        assert_eq!(pt.text(), "he?lo?a");
        assert_eq!(pt.last_index_of('?', 3), Some(2));
        assert_eq!(pt.last_index_of('x', 6), None);
        assert_eq!(pt.last_index_of('?', 6), Some(5));
    }

    #[test]
    fn char_at() {
        let mut pt = PieceTable::with_text("helo?a");
        pt.insert_range("?", 2);
        println!("{:?}", pt);
        let tx = pt.text();
        for (i, c) in tx.chars().enumerate() {
            assert_eq!(pt.char_at(i).unwrap(), c, "i = {}", i);
        }
    }

    #[test]
    fn char_iter() {
        let mut pt = PieceTable::with_text("helo?a");
        pt.insert_range("?", 2);
        println!("{:?}", pt);
        let tx = pt.text();
        for (a, b) in tx.chars().zip(pt.chars(0)) {
            println!("{} - {}", a, b);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn char_iter_back() {
        let mut pt = PieceTable::with_text("helo?a");
        pt.insert_range("?", 2);
        println!("{:?}", pt);
        let tx = pt.text();
        let mut tc = tx.chars();
        let mut ch = pt.chars(tx.len()-1);
        loop {
            let a = tc.next_back();
            let b = ch.next_back();
            println!("{:?} - {:?}", a, b);
            assert_eq!(a, b);
            if a.is_none() && b.is_none() { break; }
        }
    }
}

