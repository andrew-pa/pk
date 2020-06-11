use super::*;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum CharClass {
    Whitespace,
    Punctuation,
    Regular
}

trait CharClassify {
    fn class(self) -> CharClass;
}

impl CharClassify for char {
    fn class(self) -> CharClass {
        if self.is_whitespace() || self.is_ascii_whitespace() {
            CharClass::Whitespace
        } else if !self.is_alphanumeric() && self != '_' {
            CharClass::Punctuation
        } else {
            CharClass::Regular
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TextObject {
    Word, BigWord, Paragraph, Block(char)
}

fn matching_block_char(c: char) -> char {
    match c {
        '{' => '}',
        '(' => ')',
        '[' => ']',
        '<' => '>',
        '"' => '"',
        '\'' => '\'',
        _ => panic!("no matching block char for {}", c)
    }
}

impl TextObject {
    fn range(&self, buf: &Buffer, count: usize, include: bool) -> Range<usize> {
        // include = true->An, false->Inner
        match self {
            TextObject::Word | TextObject::BigWord => {
                let bigword = *self == TextObject::BigWord;
                let mut range = buf.cursor_index..buf.cursor_index;
                // find start of range
                let mut chars = buf.text.chars(range.start)
                    .rev()
                    .map(CharClassify::class)
                    .map(|cc| if bigword && cc == CharClass::Punctuation { CharClass::Regular } else { cc })
                    .peekable();
                let starting_class = chars.next().unwrap();
                while chars.peek().map(|cc| *cc == starting_class).unwrap_or(false) {
                    range.start -= 1;
                    chars.next();
                }
                // find end of range
                if !include && starting_class == CharClass::Whitespace { return range; }
                range.end = range.start;
                let mut chars = buf.text.chars(range.end)
                    .map(CharClassify::class)
                    .map(|cc| if bigword && cc == CharClass::Punctuation { CharClass::Regular } else { cc })
                    .peekable();
                for i in 0..count {
                    while chars.peek().map(|cc| *cc == starting_class).unwrap_or(false) {
                        range.end += 1;
                        chars.next();
                    }
                    if i > 0 || include {
                        let class = if starting_class == CharClass::Whitespace {
                                        *chars.peek().unwrap()
                                    } else { CharClass::Whitespace };
                        while chars.peek().map(|cc| *cc == class).unwrap_or(false) {
                            range.end += 1;
                            chars.next();
                        }
                    }
                }
                range.end -= 1;
                range
            },
            TextObject::Block(open_char) => {
                println!("---");
                let mut range = buf.cursor_index..buf.cursor_index;

                let mut chars = buf.text.chars(range.start).rev().peekable();
                while chars.peek().map(|cc| cc != open_char).unwrap_or(false) {
                    chars.next();
                    range.start -= 1;
                }
                if !include { range.start += 1; }

                let close_char = matching_block_char(*open_char);
                let mut chars = buf.text.chars(range.end).peekable();
                let mut count = 0;
                loop {
                    match chars.peek() {
                        Some(ch) if *ch == *open_char => {
                            println!("+");
                            count += 1;
                        }
                        Some(ch) if *ch == close_char => {
                            if count == 0 {
                                break;
                            } else {
                                println!("-");
                                count -= 1;
                            }
                        },
                        Some(_) => { }
                        None => break
                    }
                    range.end += 1;
                    println!("{:?}",chars.next());
                }
                /*while chars.peek().map(|cc| *cc != mc).unwrap_or(false) {
                    chars.next();
                    range.end += 1;
                }*/
                if !include { range.end -= 1; }

                range
            },
            _ => unimplemented!()
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MotionType {
    Char(Direction),
    Word(Direction), // words
    BigWord(Direction), // WORDS
    EndOfWord(Direction),
    EndOfBigWord(Direction),
    NextChar {
        c: char,
        place_before: bool,
        direction: Direction
    },
    RepeatNextChar {
        opposite: bool // true -> reverse direction
    },
    WholeLine,
    Line(Direction),
    StartOfLine,
    EndOfLine,
    Paragraph,
    An(TextObject),
    Inner(TextObject)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Motion {
    pub count: usize,
    pub mo: MotionType,
}

pub fn take_number(schars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
    if schars.peek().map(|c| c.is_digit(10)).unwrap_or(false) {
        let mut num = schars.next().unwrap().to_digit(10).unwrap() as usize;
        while schars.peek().map(|c| c.is_digit(10)).unwrap_or(false) {
            num = num*10 + schars.next().unwrap().to_digit(10).unwrap() as usize;  
        }
        Some(num)
    } else {
        None
    }
}

impl Motion {
    pub fn parse(c: &mut std::iter::Peekable<std::str::Chars>, opchar: Option<char>, wholecmd: &str) -> Result<Motion, Error> {
       let count = take_number(c);
       let txo = match c.peek() {
           Some('h') => MotionType::Char(Direction::Backward),
           Some('j') => MotionType::Line(Direction::Forward),
           Some('k') => MotionType::Line(Direction::Backward),
           Some('l') => MotionType::Char(Direction::Forward),
           Some('w') => MotionType::Word(Direction::Forward),
           Some('b') => MotionType::Word(Direction::Backward),
           Some('W') => MotionType::BigWord(Direction::Forward),
           Some('B') => MotionType::BigWord(Direction::Backward),
           Some('e') => MotionType::EndOfWord(Direction::Forward),
           Some('E') => MotionType::EndOfBigWord(Direction::Forward),
           Some('g') => {
               c.next();
               match c.peek() {
                   Some('e') => MotionType::EndOfWord(Direction::Backward),
                   Some('E') => MotionType::EndOfBigWord(Direction::Backward),
                   Some(_) => return Err(Error::UnknownCommand(String::from(wholecmd))),
                   None => return Err(Error::IncompleteCommand)
               }
           },
           Some('^') => MotionType::StartOfLine,
           Some('$') => MotionType::EndOfLine,
           Some('_') => MotionType::WholeLine,
           Some(&tc) if tc == 'f' || tc == 'F' || tc == 't' || tc == 'T' => {
               c.next();
               MotionType::NextChar {
                   c: c.next().ok_or(Error::IncompleteCommand)?,
                   place_before: match tc {
                       'f' => false,
                       'F' => false,
                       't' => true,
                       'T' => true,
                       _ => unreachable!()
                   },
                   direction: match tc {
                       'f' => Direction::Forward,
                       'F' => Direction::Backward,
                       't' => Direction::Forward,
                       'T' => Direction::Backward,
                       _ => unreachable!()
                   }
               }
           },
           Some('i') | Some('a') if opchar.is_some() => {
               let t = c.next();
               let obj = match c.peek() {
                   Some('w') => TextObject::Word,
                   Some('W') => TextObject::BigWord,
                   Some('p') => TextObject::Paragraph,
                   Some('{') | Some('}') => TextObject::Block('{'),
                   Some('(') | Some(')') => TextObject::Block('('),
                   Some('[') | Some(']') => TextObject::Block('['),
                   Some('<') | Some('>') => TextObject::Block('<'),
                   Some('"')  => TextObject::Block('"'),
                   Some('\'') => TextObject::Block('\''),
                   Some(_) => return Err(Error::UnknownCommand(String::from(wholecmd))),
                   None => return Err(Error::IncompleteCommand)
               };
               match t {
                   Some('i') => MotionType::Inner(obj),
                   Some('a') => MotionType::An(obj),
                   _ => unreachable!()
               }
           },
           Some(';') => MotionType::RepeatNextChar { opposite: true },
           Some(c) if opchar.map(|opc| opc == *c).unwrap_or(false)
               => MotionType::WholeLine,
           Some(_) => return Err(Error::UnknownCommand(String::from(wholecmd))),
           None => return Err(Error::IncompleteCommand)
       };
       c.next();
       Ok(Motion {
           count: count.unwrap_or(1),
           mo: txo,
       })
    }

    pub fn range(&self, buf: &Buffer) -> Range<usize> {
        let mut range = buf.cursor_index..buf.cursor_index;
        match &self.mo {
            MotionType::An(obj) => {
                return obj.range(buf, self.count, true);
            },
            MotionType::Inner(obj) => {
                return obj.range(buf, self.count, false);
            },
            _ => {}
        };
        for _ in 0..self.count {
            match &self.mo {
                MotionType::Char(Direction::Forward) => { range.end += 1 }
                MotionType::Char(Direction::Backward) => { range.end -= 1 }
                MotionType::Line(direction) => {
                    let new_line_index = match direction {
                        Direction::Forward => buf.next_line_index(range.end),
                        Direction::Backward => buf.last_line_index(range.end)
                    };
                    // probably should unwrap to the end of the buffer
                    let line_len = buf.text.index_of('\n', new_line_index).unwrap_or(0)-new_line_index;
                    range.end = buf.current_column().min(line_len)+new_line_index;
                },
                MotionType::StartOfLine => {
                    range.end = buf.current_start_of_line(range.end);
                    let mut chars = buf.text.chars(range.end).map(CharClassify::class).peekable();
                    while chars.peek().map_or(false, |cc| *cc == CharClass::Whitespace) {
                        range.end += 1;
                        chars.next();
                    }
                },
                MotionType::EndOfLine => {
                    range.end = buf.next_line_index(range.end)-1;
                }
                MotionType::Word(Direction::Forward) => {
                    // is the character under the cursor alphanumeric+ or a 'other non-blank'?
                    if buf.text.char_at(range.end).map(|c| c.is_alphanumeric()||c=='_').unwrap_or(false) {
                        // find the next whitespace or non-blank char
                        let f = buf.text.index_of_pred(|sc| !(sc.is_alphanumeric() || sc == '_'), range.end)
                            .unwrap_or(range.end);
                        // println!("F{}",f);
                        // the next word starts at either `f` or if `f` is whitespace, the next
                        // non-blank after `f`
                        range.end = if buf.text.char_at(f).map(|c| c.is_ascii_whitespace()).unwrap_or(false) {
                            // println!("G");
                            buf.text.index_of_pred(|sc| !sc.is_ascii_whitespace(), f).unwrap_or(f)
                        } else { f };
                    } else { // "a sequence of other non-blank characters"
                        // find the next blank or alphanumeric+ char
                        let f = buf.text.index_of_pred(|sc| sc.is_ascii_whitespace() || sc.is_alphanumeric() || sc == '_',
                            range.end+1).unwrap_or(range.end);
                        // the next word starts at `f` or if `f` is whitespace, at the next
                        // non-blank char after `f`
                        range.end = if buf.text.char_at(f).map(|c| c.is_ascii_whitespace()).unwrap_or(false) {
                            // println!("G");
                            buf.text.index_of_pred(|sc| !sc.is_ascii_whitespace(), f).unwrap_or(f)
                        } else { f };
                    }
                },

                MotionType::Word(Direction::Backward) => {
                    let mut chars = buf.text.chars(range.end).rev()
                        .map(CharClassify::class)
                        .peekable();
                    if let Some(_) = chars.next() {
                        range.end -= 1;
                        while let Some(CharClass::Whitespace) = chars.peek() {
                            chars.next();
                            range.end -= 1;
                        }
                        let scls = chars.peek().cloned().unwrap();
                        while range.end > 0 && chars.next().map_or(false, |x| x == scls) {
                            range.end -= 1;
                        }
                        if range.end > 0 { range.end += 1; }
                    } else {
                        range.end = 0;
                    }
                },

                MotionType::BigWord(direction) => {
                    let next_blank = buf.text.dir_index_of(|sc| sc.is_ascii_whitespace(), range.start, *direction)
                                                .unwrap_or(range.start);
                    range.end = match *direction {
                        Direction::Forward =>
                            buf.text.index_of_pred(|sc| !sc.is_ascii_whitespace(), next_blank).unwrap_or(next_blank),
                        Direction::Backward =>
                            buf.text.last_index_of_pred(|sc| sc.is_ascii_whitespace(), next_blank).map(|i| i+1).unwrap_or(0)
                    };
                },

                MotionType::EndOfWord(Direction::Forward) | MotionType::EndOfBigWord(Direction::Forward) => {
                    let mut chars: Box<dyn Iterator<Item=CharClass>> = Box::new(buf.text.chars(range.end)
                        .map(CharClassify::class));
                    if let MotionType::EndOfBigWord(_) = self.mo {
                        chars = Box::new(chars.map(|c| match c { 
                            CharClass::Punctuation => CharClass::Regular,
                            _ => c
                        }));
                    }
                    let mut chars = chars.peekable();
                    if let Some(starting_class) = chars.next() {
                        range.end += 1;
                        if starting_class != CharClass::Whitespace &&
                            chars.peek().map(|cc| *cc == starting_class).unwrap_or(false)
                        {
                            while chars.next().map_or(false, |cc| cc == starting_class) {
                                range.end += 1;
                            }
                        } else {
                            while let Some(CharClass::Whitespace) = chars.peek() {
                                chars.next();
                                range.end += 1;
                            }
                            let scls = chars.peek().cloned().unwrap();
                            while range.end < buf.text.len() &&
                                    chars.next().map_or(false, |x| x == scls)
                            {
                                range.end += 1;
                            }
                        }
                        range.end -= 1;
                    } else {
                        range.end = 0;
                    }
                },

                // of course, the most arcane is the simplest
                MotionType::EndOfWord(Direction::Backward) | MotionType::EndOfBigWord(Direction::Backward) => {
                    let mut chars: Box<dyn Iterator<Item=CharClass>> = Box::new(buf.text.chars(range.end).rev()
                        .map(CharClassify::class));
                    if let MotionType::EndOfBigWord(_) = self.mo {
                        chars = Box::new(chars.map(|c| match c { 
                            CharClass::Punctuation => CharClass::Regular,
                            _ => c
                        }));
                    }
                    let mut chars = chars.peekable();
                    if let Some(starting_class) = chars.next() {
                        range.end -= 1;
                        if starting_class != CharClass::Whitespace {
                            while chars.peek().map_or(false, |cc| *cc == starting_class) {
                                chars.next();
                                range.end -= 1;
                            }
                        }

                        while chars.peek().map_or(false, |cc| *cc == CharClass::Whitespace) {
                            chars.next();
                            range.end -= 1;
                        }
                    } else {
                        range.end = 0;
                    }
                },

                MotionType::NextChar { c, place_before, direction } => {
                    range.end = buf.text.dir_index_of(|cc| cc == *c, match direction {
                        Direction::Forward => range.end+1,
                        Direction::Backward => range.end-1
                    }, *direction).unwrap_or(range.end);
                    if *place_before {
                        match direction {
                            Direction::Forward => range.end -= 1,
                            Direction::Backward => range.end += 1,
                        }
                    }
                },

                MotionType::WholeLine => {
                    range.start = buf.current_start_of_line(range.start);
                    range.end = buf.next_line_index(range.end);
                },

                _ => unimplemented!()
            }
        }
        range
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_line_test_buffer() -> Buffer {
        let mut b = Buffer::with_text("abc\ndef\nghi\n");
        b.cursor_index = b.next_line_index(b.cursor_index);
        assert_eq!(b.cursor_index, 4);
        b
    }
    fn create_word_test_buffer() -> Buffer {
        Buffer::with_text("word\nw0rd w##d ++++ word\n")
    }

    #[test]
    fn txo_char() {
        let b = create_line_test_buffer();
        let mo = Motion {
            mo: MotionType::Char(Direction::Forward),
            count: 1
        };
        assert_eq!(mo.range(&b), 4..5);
    }

    #[test]
    fn txo_line() {

        let b = create_line_test_buffer();
        let mo = Motion {
            mo: MotionType::Line(Direction::Forward),
            count: 1
        };
        assert_eq!(mo.range(&b), 4..8);
    }
 
    #[test]
    fn txo_start_of_line() {
        let b = create_line_test_buffer();
        let mo = Motion {
            mo: MotionType::StartOfLine,
            count: 1
        };
        assert_eq!(mo.range(&b), 4..4);
    }      
 
    #[test]
    fn txo_end_of_line() {
        let b = create_line_test_buffer();
        let mo = Motion {
            mo: MotionType::EndOfLine,
            count: 1
        };
        assert_eq!(mo.range(&b), 4..7);
    }      

    #[test]
    fn txo_line_backward() {
        let b = create_line_test_buffer();
        let mo = Motion {
            mo: MotionType::Line(Direction::Backward),
            count: 1
        };
        assert_eq!(mo.range(&b), 4..0);
    }

    fn run_repeated_test<'a>(b: &mut Buffer, mo: &Motion, 
                        correct_ends: impl Iterator<Item=&'a usize>, assert_msg: &str) {
        for (i, cwb) in correct_ends.enumerate() {
            let r = mo.range(&b);
            assert_eq!(r.end, *cwb, "{} i={}", assert_msg, i);
            b.cursor_index = r.end;
        }
    }

    fn run_repeated_test_then_offset<'a>(b: &mut Buffer, mo: &Motion, 
                        correct_ends: impl Iterator<Item=&'a usize>, offset: isize, assert_msg: &str) {
        for (i, cwb) in correct_ends.enumerate() {
            let r = mo.range(&b);
            assert_eq!(r.end, *cwb, "{} i={}", assert_msg, i);
            b.cursor_index = (r.end as isize + offset) as usize;
        }
    }

    #[test]
    fn txo_word_no_spaces() {
        let mut b = Buffer::with_text("word+++word+++ +ope");
        let mo = Motion {
            mo: MotionType::Word(Direction::Forward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [4,7,11,15].iter(), "forward");
        let mo = Motion {
            mo: MotionType::Word(Direction::Backward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [11,7,4,0].iter(), "backward");
    
    }

    #[test]
    fn txo_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            mo: MotionType::Word(Direction::Forward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [5,10,11,13,15,20].iter(), "forward");

        let mo = Motion {
            mo: MotionType::Word(Direction::Backward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [15,13,11,10,5,0].iter(), "backward");
    }

    #[test]
    fn txo_big_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            mo: MotionType::BigWord(Direction::Forward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [5,10,15].iter(), "forward");

        let mo = Motion {
            mo: MotionType::BigWord(Direction::Backward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [10,5,0].iter(), "backward");
    }

    #[test]
    fn txo_end_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            mo: MotionType::EndOfWord(Direction::Forward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [3,8,10,12,13,18,23].iter(), "forward");

        let mo = Motion {
            mo: MotionType::EndOfWord(Direction::Backward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [18,13,12,10,8,3].iter(), "backward");
    }

    #[test]
    fn txo_end_big_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            mo: MotionType::EndOfBigWord(Direction::Forward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [3,8,13,18,23].iter(), "forward");

        let mo = Motion {
            mo: MotionType::EndOfBigWord(Direction::Backward),
            count: 1
        };
        run_repeated_test(&mut b, &mo, [18,13,8,3].iter(), "backward");
    }

    #[test]
    fn txo_find_next_on() {
        let mut b = Buffer::with_text("so!me s!ample tex!t");
        let correct = [2,7,17];
        let mo = Motion {
            mo: MotionType::NextChar {
                c: '!', place_before: false, direction: Direction::Forward
            },
            count: 1
        };
        run_repeated_test(&mut b, &mo, correct.iter(), "forward, place on");

        let mo = Motion {
            mo: MotionType::NextChar {
                c: '!', place_before: false, direction: Direction::Backward
            },
            count: 1
        };
        run_repeated_test(&mut b, &mo, correct.iter().rev().skip(1), "backward, place on");
    }

    #[test]
    fn txo_find_next_before() {
        let mut b = Buffer::with_text("so!me s!ample tex!t");
        let mo = Motion {
            mo: MotionType::NextChar {
                c: '!', place_before: true, direction: Direction::Forward
            },
            count: 1
        };
        run_repeated_test_then_offset(&mut b, &mo, [1,6,16].iter(), 1, "forward, place before");

        let mo = Motion {
            mo: MotionType::NextChar {
                c: '!', place_before: true, direction: Direction::Backward
            },
            count: 1
        };
        run_repeated_test_then_offset(&mut b, &mo, [8,3].iter(), -1, "backward, place before");
    }

    #[test]
    fn txo_object_a_word() {
        let mut b = Buffer::with_text(" word   w0rd wr+d");
        b.cursor_index = 3;
        let mut mo = Motion {
            mo: MotionType::An(TextObject::Word), count: 1
        };
        assert_eq!(mo.range(&b), 1..7);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..12);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..14);

        b.cursor_index = 6;
        mo.count = 1;
        assert_eq!(mo.range(&b), 5..11);
    }

    #[test]
    fn txo_object_inner_word() {
        let mut b = Buffer::with_text(" word  word+ ");
        b.cursor_index = 3;
        let mut mo = Motion {
            mo: MotionType::Inner(TextObject::Word), count: 1
        };
        assert_eq!(mo.range(&b), 1..4);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..6);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..10);

        b.cursor_index = 6;
        mo.count = 1;
        assert_eq!(mo.range(&b), 5..6);
    }

    #[test]
    fn txo_object_a_bigword() {
        let mut b = Buffer::with_text(" wor+   w0rd wr+d");
        b.cursor_index = 3;
        let mut mo = Motion {
            mo: MotionType::An(TextObject::BigWord), count: 1
        };
        assert_eq!(mo.range(&b), 1..7);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..12);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..16);

        b.cursor_index = 6;
        mo.count = 1;
        assert_eq!(mo.range(&b), 5..11);
    }

    #[test]
    fn txo_object_inner_bigword() {
        let mut b = Buffer::with_text(" w--d  w--d+ ");
        b.cursor_index = 3;
        let mut mo = Motion {
            mo: MotionType::Inner(TextObject::BigWord), count: 1
        };
        assert_eq!(mo.range(&b), 1..4);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..6);
        mo.count += 1;
        assert_eq!(mo.range(&b), 1..12); // this doesn't quite agree with Vim, but it seems questionable either way

        b.cursor_index = 6;
        mo.count = 1;
        assert_eq!(mo.range(&b), 5..6);
    }

    #[test]
    fn txo_object_a_block() {
        let mut b = Buffer::with_text("<(bl(o)ck) {\nblock\n}>");
        let mut mo = Motion {
            mo: MotionType::An(TextObject::Block('<')), count: 1
        };

        assert_eq!(mo.range(&b), 0..20, "on <");
        b.cursor_index += 3;
        assert_eq!(mo.range(&b), 0..20, "in <");

        b.cursor_index = 1;
        mo.mo = MotionType::An(TextObject::Block('('));
        assert_eq!(mo.range(&b), 1..9, "on first (");
        b.cursor_index += 2;
        assert_eq!(mo.range(&b), 1..9, "in first (");

        b.cursor_index += 2;
        assert_eq!(mo.range(&b), 4..6, "in nested (");

        b.cursor_index = 15;
        mo.mo = MotionType::An(TextObject::Block('{'));
        assert_eq!(mo.range(&b), 11..19, "in {{");
    }

    #[test]
    fn txo_object_inner_block() {
        let mut b = Buffer::with_text("<(bl(o)ck) {\nblock\n}>");
        let mut mo = Motion {
            mo: MotionType::Inner(TextObject::Block('<')), count: 1
        };

        assert_eq!(mo.range(&b), 1..19, "on <");
        b.cursor_index += 3;
        assert_eq!(mo.range(&b), 1..19, "in <");

        b.cursor_index = 1;
        mo.mo = MotionType::Inner(TextObject::Block('('));
        assert_eq!(mo.range(&b), 2..8, "on first (");
        b.cursor_index += 2;
        assert_eq!(mo.range(&b), 2..8, "in first (");

        b.cursor_index += 2;
        assert_eq!(mo.range(&b), 5..5, "in nested (");

        b.cursor_index = 15;
        mo.mo = MotionType::Inner(TextObject::Block('{'));
        assert_eq!(mo.range(&b), 12..18, "in {{");
    }


}

