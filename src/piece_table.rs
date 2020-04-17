#![allow(dead_code)]
#![allow(unused_variables)]

#[derive(Copy,Clone,Debug)]
struct Piece {
    source: usize,
    start: usize, length: usize
}

impl Piece {
    /// `at` is relative to the beginning of the piece
    fn split(self, at: usize) -> (Piece, Piece) {
        (Piece { source: self.source, start: self.start, length: at },
         Piece { source: self.source, start: self.start+at, length: self.length-at })
    }
}

#[derive(Debug)]
enum Change {
    Insert {
        piece_index: usize,
    },
    Modify {
        piece_index: usize,
        old: Piece
    },
    Delete {
        piece_index: usize,
        old: Piece
    }
}

type Action=Vec<Change>;

#[derive(Debug)]
pub struct PieceTable {
    sources: Vec<String>,
    pieces: Vec<Piece>,
    history: Vec<Action>
}

impl Default for PieceTable {
    fn default() -> PieceTable {
        PieceTable {
            sources: vec![String::from("Hello, world!")],
            pieces: vec![ Piece { source: 0, start: 0, length: 13 } ],
            history: Vec::new()
        }
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

impl<'table> PieceTable {
    pub fn with_text(s: &str) -> PieceTable {
        PieceTable {
            sources: vec![s.to_string()],
            pieces: vec![ Piece { source: 0, start: 0, length: s.len() } ],
            history: Vec::new()
        }
    }

    fn reverse_change(&mut self, change: &Change) {
        match *change {
            Change::Insert { piece_index } => {
                self.pieces.remove(piece_index);
            },
            Change::Modify { piece_index, old } => {
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
        let mut action = Action::new();
        for (i,p) in self.pieces.iter().enumerate() {
            println!("{} {:?}", i, p);
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    self.pieces.insert(i, new_piece);
                    action.push(Change::Insert{piece_index: i});
                } else if index == ix+p.length { // we're inserting at the end of this piece
                    self.pieces.insert(i+1, new_piece);
                    action.push(Change::Insert{piece_index: i+1});
                } else { //insertion in the middle
                    let (left,right) = p.split(index-ix);
                    action.push(Change::Modify{piece_index: i, old: p.clone()});
                    action.push(Change::Insert{piece_index: i+1});
                    action.push(Change::Insert{piece_index: i+2});
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
        let mut action = Action::new();
        for (i,p) in self.pieces.iter().enumerate() {
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    self.pieces.insert(i, Piece { source: self.sources.len(), start: 0, length: 0 });
                    self.sources.push(String::new());
                    action.push(Change::Insert { piece_index: i });
                    insertion_piece_index = Some(i);
                } else if index == ix+p.length-1 { // we're inserting at the end of this piece
                    if self.sources[p.source].len() ==
                        p.start+p.length { // we're inserting at the current end of the piece in the source
                        insertion_piece_index = Some(i);
                    } else {
                        self.pieces.insert(i+1, Piece { source: self.sources.len(), start: 0, length: 0 });
                        action.push(Change::Insert { piece_index: i+1 });
                        self.sources.push(String::new());
                        insertion_piece_index = Some(i+1);
                    }
                } else { //insertion in the middle
                    let (a,c) = p.split(index-ix);
                    let b = Piece { source: self.sources.len(), start: 0, length: 0 };
                    self.sources.push(String::new());
                    action.push(Change::Modify { piece_index: i, old: p.clone() });
                    action.push(Change::Insert { piece_index: i+1 });
                    action.push(Change::Insert { piece_index: i+2 });
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

    pub fn delete_range(&mut self, start: usize, end: usize) {
        assert!(start != end);

        let mut start_piece: Option<(usize,usize)> = None;
        let mut end_piece:   Option<(usize,usize)> = None;
        let mut mid_pieces:  Vec<usize>            = Vec::new();
        let mut global_index                       = 0usize;
        let mut action = Action::new();

        for (i,p) in self.pieces.iter().enumerate() {
            if start < global_index && end >= global_index+p.length {
                mid_pieces.push(i);
            } else if start >= global_index && start < global_index+p.length {
                if end > global_index && end <= global_index+p.length {
                    // this piece totally contains this range
                    if start-global_index == 0 && end-global_index == p.length {
                        action.push(Change::Delete { piece_index: i, old: self.pieces.remove(i) });
                    } else {
                        let (left_keep, deleted_right) = p.split(start-global_index);
                        let (_deleted, right_keep) = deleted_right.split(end-(global_index+left_keep.length-1));
                        action.push(Change::Modify { piece_index: i, old: p.clone() });
                        action.push(Change::Insert { piece_index: i+1 });
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

        action.push(Change::Modify { piece_index: start_piece, old: self.pieces[start_piece] });
        action.push(Change::Modify { piece_index: end_piece, old: self.pieces[end_piece] });

        self.pieces[start_piece] = self.pieces[start_piece].split(start_cut).0;
        self.pieces[end_piece]   = self.pieces[end_piece].split(end_cut).1;

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

    pub fn text(&self) -> String {
        let len = self.pieces.iter().fold(0, |a,p| a+p.length);
        let mut s = String::with_capacity(len);
        for piece in self.pieces.iter() {
           s.push_str(&self.sources[piece.source][piece.start..(piece.start+piece.length)]); 
        }
        s
    }
}

pub struct PieceTableRenderer {
    fnt: Font,
    pub cursor_index: usize
}

use runic::*;
impl PieceTableRenderer {
    pub fn init(rx: &mut RenderContext, fnt: Font) -> Self {
        PieceTableRenderer { fnt, cursor_index: 0 }
    }

    pub fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable, position: Point) {
        rx.set_color(Color::white());
        let mut global_index = 0usize;
        let mut cur_pos = position; 
        let mut line_num = 0usize;
        for p in table.pieces.iter() {
            let src = &table.sources[p.source][p.start..(p.start+p.length)];
            let mut lni = src.lines().peekable(); 
            loop {
                let ln = lni.next();
                if ln.is_none() { break; }
                let ln = ln.unwrap();
                let layout = rx.new_text_layout(ln, &self.fnt, 10000.0, 10000.0).expect("create text layout");
                rx.draw_text_layout(cur_pos, &layout);
                //rx.draw_text(Rect::pnwh(cur_pos - Point::x(12.0), 100.0, 100.0), &format!("{}", global_index), &self.fnt);
                if self.cursor_index >= global_index && self.cursor_index <= global_index+ln.len() {
                    //fix: the cursor doesn't appear at the end of lines because DWrite returns a
                    // zero-area rectangle, so that case needs to be checked for and a cursor
                    // rectangle manually computed
                    let curbounds = layout.char_bounds(self.cursor_index - global_index);
                    //rx.fill_rect(Rect::xywh(0.0,0.0,8.0,8.0));
                    rx.stroke_rect(curbounds.offset(cur_pos), 2.0);
                }
                let text_size = layout.bounds();
                cur_pos.x += text_size.w; 
                global_index += ln.len();
                if let Some(_) = lni.peek() {
                    // new line
                    line_num+=1;
                    cur_pos.x = position.x;
                    cur_pos.y += text_size.h;
                    global_index += 1;
                } else {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
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
        assert_eq!(pt.text(), "hlo");
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


}

struct TextRenderer {
    fnt: Font
}

impl TextRenderer {
    fn init(rx: &mut RenderContext, fnt: Font) -> Self {
        TextRenderer { fnt }
    }

    fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable) {
        rx.set_color(Color::white());
        rx.draw_text(Rect::xywh(32.0,32.0,128.0,64.0), "Hello, world! ==>", &self.fnt);
    }
}

