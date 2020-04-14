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
pub struct PieceTable {
    sources: Vec<String>,
    pieces: Vec<Piece>
}

impl Default for PieceTable {
    fn default() -> PieceTable {
        PieceTable {
            sources: vec![String::new()],
            pieces: vec![ Piece { source: 0, start: 0, length: 0 } ]
        }
    }
}

struct TableMutator {
    piece_ix: usize
}
impl TableMutator {
    fn push_char(&mut self, pt: &mut PieceTable, c: char) {
        pt.pieces[self.piece_ix].length += 1;
        let si = pt.pieces[self.piece_ix].source;
        pt.sources[si].push(c);
    }

    fn pop_char(&mut self, pt: &mut PieceTable) -> bool {
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
    fn with_text(s: &str) -> PieceTable {
        PieceTable {
            sources: vec![s.to_string()],
            pieces: vec![ Piece { source: 0, start: 0, length: s.len() } ]
        }
    }

    fn insert_range(&mut self, s: &str, index: usize) {
        let mut ix = 0usize;
        let new_piece = Piece { source: self.sources.len(), start: 0, length: s.len() };
        self.sources.push(String::from(s));
        for (i,p) in self.pieces.iter().enumerate() {
            println!("{} {:?}", i, p);
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    self.pieces.insert(i, new_piece);
                } else if index == ix+p.length { // we're inserting at the end of this piece
                    self.pieces.insert(i+1, new_piece);
                } else { //insertion in the middle
                    let (left,right) = p.split(index-ix);
                    self.pieces[i] = left;
                    self.pieces.insert(i+1, new_piece);
                    self.pieces.insert(i+2, right);
                }
                break;
            }
            ix += p.length;
        }
    }

    fn insert_mutator(&mut self, index: usize) -> TableMutator {
        let mut ix = 0usize;
        let mut insertion_piece_index: Option<usize> = None;
        for (i,p) in self.pieces.iter().enumerate() {
            if index >= ix && index < ix+p.length {
                if index == ix { // we're inserting at the start of this piece
                    self.pieces.insert(i, Piece { source: self.sources.len(), start: 0, length: 0 });
                    self.sources.push(String::new());
                    insertion_piece_index = Some(i);
                } else if index == ix+p.length-1 { // we're inserting at the end of this piece
                    if self.sources[p.source].len() ==
                        p.start+p.length { // we're inserting at the current end of the piece in the source
                        insertion_piece_index = Some(i);
                    } else {
                        self.pieces.insert(i+1, Piece { source: self.sources.len(), start: 0, length: 0 });
                        self.sources.push(String::new());
                        insertion_piece_index = Some(i+1);
                    }
                } else { //insertion in the middle
                    let (a,c) = p.split(index-ix);
                    let b = Piece { source: self.sources.len(), start: 0, length: 0 };
                    self.sources.push(String::new());
                    self.pieces[i] = a;
                    self.pieces.insert(i+1, b);
                    self.pieces.insert(i+2, c);
                    insertion_piece_index = Some(i+1);
                }
                break;
            }
            ix += p.length;
        }
        TableMutator { piece_ix: insertion_piece_index.unwrap() }
    }

    fn delete_range(&mut self, start: usize, end: usize) {
        assert!(start != end);

        let mut start_piece: Option<(usize,usize)> = None;
        let mut end_piece:   Option<(usize,usize)> = None;
        let mut mid_pieces:  Vec<usize>            = Vec::new();
        let mut global_index                       = 0usize;

        for (i,p) in self.pieces.iter().enumerate() {
            if start < global_index && end >= global_index+p.length {
                mid_pieces.push(i);
            } else if start >= global_index && start < global_index+p.length {
                if end > global_index && end <= global_index+p.length {
                    // this piece totally contains this range
                    if start-global_index == 0 && end-global_index == p.length {
                        self.pieces.remove(i);
                    } else {
                        let (left_keep, deleted_right) = p.split(start-global_index);
                        let (_deleted, right_keep) = deleted_right.split(end-(global_index+left_keep.length-1));
                        self.pieces[i] = left_keep;
                        self.pieces.insert(i+1, right_keep);
                    }
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

        self.pieces[start_piece] = self.pieces[start_piece].split(start_cut).0;
        self.pieces[end_piece]   = self.pieces[end_piece].split(end_cut).1;

        for i in mid_pieces {
            self.pieces.remove(i);
        }
    }

    fn copy_range(&mut self, start: usize, end: usize) -> String {
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

    fn text(&self) -> String {
        let len = self.pieces.iter().fold(0, |a,p| a+p.length);
        let mut s = String::with_capacity(len);
        for piece in self.pieces.iter() {
           s.push_str(&self.sources[piece.source][piece.start..(piece.start+piece.length)]); 
        }
        s
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
}
