
use super::*;
use std::ops::Range;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction { Forward, Backward }

#[derive(Debug, PartialEq, Eq)]
pub enum Operator {
    Repeat,
    Undo,
    Delete,
    Change,
    Yank,
    Put,
    Indent(Direction),
    MoveAndEnterMode(ModeTag),
    NewLineAndEnterMode(Direction, ModeTag),
    ReplaceChar(char)
}

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
        direction: Direction, // true -> towards the end
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
    count: usize,
    object: TextObject,
    modifier: TextObjectMod
}

fn take_number(schars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
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
    fn parse(c: &mut std::iter::Peekable<std::str::Chars>, opchar: Option<char>, wholecmd: &str) -> Result<Motion, Error> {
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

    fn range(&self, buf: &Buffer) -> Range<usize> {
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
                    let line_len = buf.text.index_of('\n', new_line_index).unwrap_or(0)-new_line_index;
                    range.end = buf.current_column().min(line_len)+new_line_index;
                }
                _ => unimplemented!()
            }
        }
        range
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModeTag {
    Normal, Insert, Command, Visual
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Move(Motion),
    Edit {
        op: Operator,
        op_count: usize,
        mo: Motion,
        target_register: char,
    },
    ChangeMode(ModeTag)
}

impl Command {
    pub fn parse(s: &str) -> Result<Command, Error> {
        let mut target_reg: Option<char> = None;
        let mut schars = s.chars().peekable();
        match schars.peek() {
            Some('i') => return Ok(Command::ChangeMode(ModeTag::Insert)),
            Some('I') => return Ok(Command::Edit {
                    op: Operator::MoveAndEnterMode(ModeTag::Insert),
                    mo: Motion { count: 1, object: TextObject::StartOfLine, modifier: TextObjectMod::None },
                    op_count: 1, target_register: '"'
            }),
            Some('a') => return Ok(Command::Edit {
                op: Operator::MoveAndEnterMode(ModeTag::Insert),
                mo: Motion { count: 1, object: TextObject::Char(Direction::Forward), modifier: TextObjectMod::None },
                op_count: 1, target_register: '"'
            }),
            Some('A') => return Ok(Command::Edit {
                op: Operator::MoveAndEnterMode(ModeTag::Insert),
                mo: Motion { count: 1, object: TextObject::EndOfLine, modifier: TextObjectMod::None },
                op_count: 1, target_register: '"'
            }),
            Some('o') => return Ok(Command::Edit {
                op: Operator::NewLineAndEnterMode(Direction::Forward, ModeTag::Insert),
                mo: Motion { count: 1, object: TextObject::Line(Direction::Forward), modifier: TextObjectMod::None },
                op_count: 1, target_register: '"'
            }),
            Some('O') => return Ok(Command::Edit {
                op: Operator::NewLineAndEnterMode(Direction::Backward, ModeTag::Insert),
                mo: Motion { count: 1, object: TextObject::Line(Direction::Backward), modifier: TextObjectMod::None },
                op_count: 1, target_register: '"'
            }),

            Some('v') => return Ok(Command::ChangeMode(ModeTag::Visual)),
            Some(':') => return Ok(Command::ChangeMode(ModeTag::Command)),
            Some('r') => {
                schars.next();
                return Ok(Command::Edit {
                    op: Operator::ReplaceChar(schars.next().ok_or(Error::IncompleteCommand)?),
                    mo: Motion { count: 0, object: TextObject::Char(Direction::Forward), modifier: TextObjectMod::None },
                    op_count: 1, target_register: '"'
                })
            },
            Some('"') => {
                schars.next();
                target_reg = schars.next();
            },
            Some(_) => {},
            None => return Err(Error::InvalidCommand(String::from(s)))
        }
        let opcount = take_number(&mut schars);
        let op = match schars.peek() {
            Some('.') => Some(Operator::Repeat),
            Some('u') => Some(Operator::Undo),
            Some('d') => Some(Operator::Delete),
            Some('c') => Some(Operator::Change),
            Some('y') => Some(Operator::Yank),
            Some('p') => Some(Operator::Put),
            Some('<') => Some(Operator::Indent(Direction::Backward)),
            Some('>') => Some(Operator::Indent(Direction::Forward)),
            Some('x') => return Ok(Command::Edit {
                op: Operator::Delete, op_count: opcount.unwrap_or(1), 
                mo: Motion { count: 1, object: TextObject::Char(Direction::Forward), modifier: TextObjectMod::None },
                target_register: target_reg.unwrap_or('"')
            }),
            Some(_) => None,
            None => None
        };
        let mut opchar = None;
        if op.is_some() { opchar = schars.next(); }
        let mut mo = Motion::parse(&mut schars, opchar, s)?;
        if op.is_some() {
            Ok(Command::Edit {
                op: op.unwrap(),
                op_count: opcount.unwrap_or(1),
                mo, target_register: target_reg.unwrap_or('"')
            })
        }
        else {
            if let Some(opc) = opcount {
                mo.count *= opc;
            }
            Ok(Command::Move(mo))
        }
    }

    pub fn execute(&self, buf: &mut Buffer) -> Result<Option<ModeTag>, Error> {
        match self {
            Command::Move(mo) => {
                let Range { start: _, end } = mo.range(buf);
                buf.cursor_index = end;
                Ok(None)
            },
            Command::Edit { op, op_count, mo, target_register } => Ok(None),
            &Command::ChangeMode(mode) => Ok(Some(mode))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn cmd_parse_correct() {
        assert_eq!(Command::parse("i").unwrap(),
            Command::ChangeMode(ModeTag::Insert));
        assert_eq!(Command::parse("x").unwrap(),
            Command::Edit{op: Operator::Delete, mo: Motion{count:1,object:TextObject::Char(Direction::Forward), modifier: TextObjectMod::None}, op_count: 1, target_register: '"'});
        assert_eq!(Command::parse("w").unwrap(),
            Command::Move(Motion { count: 1, object: TextObject::Word(Direction::Forward), modifier: TextObjectMod::None }));
        assert_eq!(Command::parse("dw").unwrap(),
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 1, object: TextObject::Word(Direction::Forward), modifier: TextObjectMod::None },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("2dw").unwrap(),
            Command::Edit{
                op: Operator::Delete, op_count: 2,
                mo: Motion { count: 1, object: TextObject::Word(Direction::Forward), modifier: TextObjectMod::None },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("d2w").unwrap(),
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 2, object: TextObject::Word(Direction::Forward), modifier: TextObjectMod::None },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("\"adw").unwrap(),
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 1, object: TextObject::Word(Direction::Forward), modifier: TextObjectMod::None },
                target_register: 'a'
            }
        );
    }

    #[test]
    fn cmd_parse_incorrect() {
        if let Error::UnknownCommand(c) = Command::parse("Z").unwrap_err() {
            assert_eq!(c, "Z");
        } else {
            panic!("expected 'Z' to be an unknown command");
        }
        if let Error::IncompleteCommand = Command::parse("d").unwrap_err() {
        } else {
            panic!("expected 'd' to be an incomplete command");
        }
        if let Error::IncompleteCommand = Command::parse("3").unwrap_err() {
        } else {
            panic!("expected '3' to be an incomplete command");
        }
        if let Error::IncompleteCommand = Command::parse("2df").unwrap_err() {
        } else {
            panic!("expected '2df' to be an incomplete command");
        }
    }
}

