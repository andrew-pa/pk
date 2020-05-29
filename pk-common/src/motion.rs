use super::*;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
pub enum TextObject {
    Char(Direction),
    Word(Direction), // words
    BigWord(Direction), // WORDS
    EndOfWord(Direction),
    EndOfBigWord(Direction),
    NextChar {
        c: char,
        place_before: bool,
        direction: Direction,
    },
    RepeatNextChar {
        opposite: bool // true -> reverse direction
    },
    WholeLine,
    Line(Direction),
    StartOfLine,
    EndOfLine,
    Paragraph
}

#[derive(Debug, PartialEq, Eq)]
pub enum TextObjectMod {
    None, AnObject, InnerObject
}

#[derive(Debug, PartialEq, Eq)]
pub struct Motion {
    pub count: usize,
    pub object: TextObject,
    pub modifier: TextObjectMod
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

impl Motion {
    pub fn parse(c: &mut std::iter::Peekable<std::str::Chars>, opchar: Option<char>, wholecmd: &str) -> Result<Motion, Error> {
       let count = take_number(c);
       let txm = match c.peek() {
           Some('i') => { c.next(); TextObjectMod::InnerObject }, 
           Some('a') => { c.next(); TextObjectMod::AnObject },
           _ => TextObjectMod::None
       };
       let txo = match c.peek() {
           Some('h') => TextObject::Char(Direction::Backward),
           Some('j') => TextObject::Line(Direction::Forward),
           Some('k') => TextObject::Line(Direction::Backward),
           Some('l') => TextObject::Char(Direction::Forward),
           Some('w') => TextObject::Word(Direction::Forward),
           Some('b') => TextObject::Word(Direction::Backward),
           Some('W') => TextObject::BigWord(Direction::Forward),
           Some('B') => TextObject::BigWord(Direction::Backward),
           Some('e') => TextObject::EndOfWord(Direction::Forward),
           Some('E') => TextObject::EndOfBigWord(Direction::Forward),
           Some('g') => {
               c.next();
               match c.peek() {
                   Some('e') => TextObject::EndOfWord(Direction::Backward),
                   Some('E') => TextObject::EndOfBigWord(Direction::Backward),
                   Some(_) => return Err(Error::UnknownCommand(String::from(wholecmd))),
                   None => return Err(Error::IncompleteCommand)
               }
           },
           Some('^') => TextObject::StartOfLine,
           Some('$') => TextObject::EndOfLine,
           Some('_') => TextObject::WholeLine,
           Some(&tc) if tc == 'f' || tc == 'F' || tc == 't' || tc == 'T' => {
               c.next();
               TextObject::NextChar {
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
           Some(';') => TextObject::RepeatNextChar { opposite: true },
           Some(c) if opchar.map(|opc| opc == *c).unwrap_or(false)
               => TextObject::WholeLine,
           Some(_) => return Err(Error::UnknownCommand(String::from(wholecmd))),
           None => return Err(Error::IncompleteCommand)
       };
       c.next();
       Ok(Motion {
           count: count.unwrap_or(1),
           object: txo,
           modifier: txm
       })
    }

    pub fn range(&self, buf: &Buffer) -> Range<usize> {
        let mut range = buf.cursor_index..buf.cursor_index;
        for _ in 0..self.count {
            match &self.object {
                TextObject::Char(Direction::Forward) => { range.end += 1 }
                TextObject::Char(Direction::Backward) => { range.end -= 1 }
                TextObject::Line(direction) => {
                    let new_line_index = match direction {
                        Direction::Forward => buf.next_line_index(range.end),
                        Direction::Backward => buf.last_line_index(range.end)
                    };
                    // probably should unwrap to the end of the buffer
                    let line_len = buf.text.index_of('\n', new_line_index).unwrap_or(0)-new_line_index;
                    range.end = buf.current_column().min(line_len)+new_line_index;
                },
                TextObject::StartOfLine => {
                    range.end = buf.current_start_of_line(range.end);
                    let mut chars = buf.text.chars(range.end).map(CharClassify::class);
                    while chars.next().map_or(false, |cc| cc == CharClass::Whitespace) {
                        range.end += 1;
                    }
                },
                TextObject::EndOfLine => {
                    range.end = buf.next_line_index(range.end)-1;
                }
                TextObject::Word(Direction::Forward) => {
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

                TextObject::Word(Direction::Backward) => {
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

                TextObject::BigWord(direction) => {
                    let next_blank = buf.text.dir_index_of(|sc| sc.is_ascii_whitespace(), range.start, *direction)
                                                .unwrap_or(range.start);
                    range.end = match *direction {
                        Direction::Forward =>
                            buf.text.index_of_pred(|sc| !sc.is_ascii_whitespace(), next_blank).unwrap_or(next_blank),
                        Direction::Backward =>
                            buf.text.last_index_of_pred(|sc| sc.is_ascii_whitespace(), next_blank).map(|i| i+1).unwrap_or(0)
                    };
                },

                TextObject::EndOfWord(Direction::Forward) | TextObject::EndOfBigWord(Direction::Forward) => {
                    let mut chars: Box<dyn Iterator<Item=CharClass>> = Box::new(buf.text.chars(range.end)
                        .map(CharClassify::class));
                    if let TextObject::EndOfBigWord(_) = self.object {
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
                TextObject::EndOfWord(Direction::Backward) | TextObject::EndOfBigWord(Direction::Backward) => {
                    let mut chars: Box<dyn Iterator<Item=CharClass>> = Box::new(buf.text.chars(range.end).rev()
                        .map(CharClassify::class));
                    if let TextObject::EndOfBigWord(_) = self.object {
                        chars = Box::new(chars.map(|c| match c { 
                            CharClass::Punctuation => CharClass::Regular,
                            _ => c
                        }));
                    }
                    let mut chars = chars.peekable();
                    if let Some(starting_class) = chars.next() {
                        range.end -= 1;
                        if(starting_class != CharClass::Whitespace) {
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

                TextObject::NextChar { c, place_before, direction } => {
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

                TextObject::WholeLine => {
                    range.start = buf.current_start_of_line(range.start);
                    range.end = buf.next_line_index(range.end);
                }


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
            object: TextObject::Char(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        assert_eq!(mo.range(&b), 4..5);
    }

    #[test]
    fn txo_line() {

        let b = create_line_test_buffer();
        let mo = Motion {
            object: TextObject::Line(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        assert_eq!(mo.range(&b), 4..8);
    }
 
    #[test]
    fn txo_start_of_line() {
        let b = create_line_test_buffer();
        let mo = Motion {
            object: TextObject::StartOfLine,
            count: 1,
            modifier: TextObjectMod::None
        };
        assert_eq!(mo.range(&b), 4..4);
    }      
 
    #[test]
    fn txo_end_of_line() {
        let b = create_line_test_buffer();
        let mo = Motion {
            object: TextObject::EndOfLine,
            count: 1,
            modifier: TextObjectMod::None
        };
        assert_eq!(mo.range(&b), 4..7);
    }      

    #[test]
    fn txo_line_backward() {
        let b = create_line_test_buffer();
        let mo = Motion {
            object: TextObject::Line(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
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
            object: TextObject::Word(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [4,7,11,15].iter(), "forward");
        let mo = Motion {
            object: TextObject::Word(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [11,7,4,0].iter(), "backward");
    
    }

    #[test]
    fn txo_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            object: TextObject::Word(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [5,10,11,13,15,20].iter(), "forward");

        let mo = Motion {
            object: TextObject::Word(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [15,13,11,10,5,0].iter(), "backward");
    }

    #[test]
    fn txo_big_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            object: TextObject::BigWord(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [5,10,15].iter(), "forward");

        let mo = Motion {
            object: TextObject::BigWord(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [10,5,0].iter(), "backward");
    }

    #[test]
    fn txo_end_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            object: TextObject::EndOfWord(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [3,8,10,12,13,18,23].iter(), "forward");

        let mo = Motion {
            object: TextObject::EndOfWord(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [18,13,12,10,8,3].iter(), "backward");
    }

    #[test]
    fn txo_end_big_word() {
        let mut b = create_word_test_buffer();
        let mo = Motion {
            object: TextObject::EndOfBigWord(Direction::Forward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [3,8,13,18,23].iter(), "forward");

        let mo = Motion {
            object: TextObject::EndOfBigWord(Direction::Backward),
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, [18,13,8,3].iter(), "backward");
    }

    #[test]
    fn txo_find_next_on() {
        let mut b = Buffer::with_text("so!me s!ample tex!t");
        let correct = [2,7,17];
        let mo = Motion {
            object: TextObject::NextChar {
                c: '!', place_before: false, direction: Direction::Forward
            },
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, correct.iter(), "forward, place on");

        let mo = Motion {
            object: TextObject::NextChar {
                c: '!', place_before: false, direction: Direction::Backward
            },
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test(&mut b, &mo, correct.iter().rev().skip(1), "backward, place on");
    }

    #[test]
    fn txo_find_next_before() {
        let mut b = Buffer::with_text("so!me s!ample tex!t");
        let mo = Motion {
            object: TextObject::NextChar {
                c: '!', place_before: true, direction: Direction::Forward
            },
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test_then_offset(&mut b, &mo, [1,6,16].iter(), 1, "forward, place before");

        let mo = Motion {
            object: TextObject::NextChar {
                c: '!', place_before: true, direction: Direction::Backward
            },
            count: 1,
            modifier: TextObjectMod::None
        };
        run_repeated_test_then_offset(&mut b, &mo, [8,3].iter(), -1, "backward, place before");
    }

}

