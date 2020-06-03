
use super::*;
use super::motion::*;
use std::ops::Range;
use std::collections::HashMap;


#[derive(Debug, PartialEq, Eq)]
pub enum Operator {
    Repeat,
    Delete,
    Change,
    Yank,
    Indent(Direction),
    MoveAndEnterMode(ModeTag),
    NewLineAndEnterMode(Direction, ModeTag),
    ReplaceChar(char)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModeTag {
    Normal, Insert, Command, Visual
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Move(Motion),
    Undo { count: usize },
    Put {
        count: usize,
        source_register: char,
        clear_register: bool
    },
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
                    mo: Motion { count: 1, mo: MotionType::StartOfLine },
                    op_count: 1, target_register: '"'
            }),
            Some('a') => return Ok(Command::Edit {
                op: Operator::MoveAndEnterMode(ModeTag::Insert),
                mo: Motion { count: 1, mo: MotionType::Char(Direction::Forward) },
                op_count: 1, target_register: '"'
            }),
            Some('A') => return Ok(Command::Edit {
                op: Operator::MoveAndEnterMode(ModeTag::Insert),
                mo: Motion { count: 1, mo: MotionType::EndOfLine },
                op_count: 1, target_register: '"'
            }),
            Some('o') => return Ok(Command::Edit {
                op: Operator::NewLineAndEnterMode(Direction::Forward, ModeTag::Insert),
                mo: Motion { count: 1, mo: MotionType::Line(Direction::Forward) },
                op_count: 1, target_register: '"'
            }),
            Some('O') => return Ok(Command::Edit {
                op: Operator::NewLineAndEnterMode(Direction::Backward, ModeTag::Insert),
                mo: Motion { count: 1, mo: MotionType::Line(Direction::Backward) },
                op_count: 1, target_register: '"'
            }),

            Some('v') => return Ok(Command::ChangeMode(ModeTag::Visual)),
            Some(':') => return Ok(Command::ChangeMode(ModeTag::Command)),
            Some('r') => {
                schars.next();
                return Ok(Command::Edit {
                    op: Operator::ReplaceChar(schars.next().ok_or(Error::IncompleteCommand)?),
                    mo: Motion { count: 0, mo: MotionType::Char(Direction::Forward) },
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
            Some('u') => return Ok(Command::Undo { count: opcount.unwrap_or(1) }),
            Some('d') => Some(Operator::Delete),
            Some('c') => Some(Operator::Change),
            Some('y') => Some(Operator::Yank),
            Some('<') => Some(Operator::Indent(Direction::Backward)),
            Some('>') => Some(Operator::Indent(Direction::Forward)),
            Some('x') => return Ok(Command::Edit {
                op: Operator::Delete, op_count: opcount.unwrap_or(1), 
                mo: Motion { count: 1, mo: MotionType::Char(Direction::Forward) },
                target_register: target_reg.unwrap_or('"')
            }),
            Some('p') => return Ok(Command::Put {
                count: opcount.unwrap_or(1), 
                source_register: target_reg.unwrap_or('"'),
                clear_register: true
            }),
            Some('P') => return Ok(Command::Put {
                count: opcount.unwrap_or(1), 
                source_register: target_reg.unwrap_or('"'),
                clear_register: false
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

    pub fn execute(&self, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<ModeTag>, Error> {
        match self {
            Command::Move(mo) => {
                let Range { start: _, end } = mo.range(buf);
                buf.cursor_index = end;
                Ok(None)
            },
            Command::Put { count, source_register, clear_register } => {
                let src = registers.get(source_register).ok_or(Error::EmptyRegister(*source_register))?;
                buf.text.insert_range(src, buf.cursor_index);
                buf.cursor_index += src.len();
                if *clear_register {
                    registers.remove(source_register);
                }
                Ok(None)
            },
            Command::Undo { count } => {
                for _ in 0..*count {
                    buf.text.undo();
                }
                Ok(None)
            },
            Command::Edit { op, op_count, mo, target_register } => {
                match op {
                    Operator::Delete | Operator::Change => {
                        let mut r = mo.range(buf);
                        if let MotionType::An(_) = mo.mo {
                            r.end += 1;
                        }
                        if let MotionType::Inner(_) = mo.mo {
                            r.end += 1;
                        }
                        registers.insert(*target_register, buf.text.copy_range(r.start, r.end-1));
                        buf.text.delete_range(r.start, r.end-1);
                        buf.cursor_index = r.start;
                        Ok(if *op == Operator::Change {
                            Some(ModeTag::Insert)
                        } else {
                            None
                        })
                    },
                    Operator::Yank => {
                        let mut r = mo.range(buf);
                        if let MotionType::An(_) = mo.mo {
                            r.end += 1;
                        }
                        if let MotionType::Inner(_) = mo.mo {
                            r.end += 1;
                        }
                        registers.insert(*target_register, buf.text.copy_range(r.start, r.end-1));
                        Ok(None)
                    },
                    Operator::ReplaceChar(c) => {
                        buf.text.delete_range(buf.cursor_index, buf.cursor_index);
                        let mut m = buf.text.insert_mutator(buf.cursor_index);
                        m.push_char(&mut buf.text, *c);
                        Ok(None)
                    },
                    Operator::MoveAndEnterMode(mode) => {
                        let Range { start: _, end } = mo.range(buf);
                        buf.cursor_index = end;
                        Ok(Some(*mode))
                    },
                    Operator::NewLineAndEnterMode(dir, mode) => {
                        let idx = match dir {
                            Direction::Forward => buf.next_line_index(buf.cursor_index),
                            Direction::Backward => buf.current_start_of_line(buf.cursor_index)
                        };
                        buf.text.insert_range("\n", idx);
                        buf.cursor_index = idx;
                        Ok(Some(*mode))
                    }
                    _ => unimplemented!()
                }
            },
            &Command::ChangeMode(mode) => Ok(Some(mode))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cmd_parse_correct() -> Result<(), Error> {
        assert_eq!(Command::parse("i")?,
            Command::ChangeMode(ModeTag::Insert));
        assert_eq!(Command::parse("x")?,
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion{count:1, mo: MotionType::Char(Direction::Forward) },
                target_register: '"'
            });
        assert_eq!(Command::parse("w")?,
            Command::Move(Motion { count: 1, mo: MotionType::Word(Direction::Forward) }));
        assert_eq!(Command::parse("dw")?,
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 1, mo: MotionType::Word(Direction::Forward) },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("2dw")?,
            Command::Edit{
                op: Operator::Delete, op_count: 2,
                mo: Motion { count: 1, mo: MotionType::Word(Direction::Forward) },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("d2w")?,
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 2, mo: MotionType::Word(Direction::Forward) },
                target_register: '"'
            }
        );
        assert_eq!(Command::parse("\"adw")?,
            Command::Edit{
                op: Operator::Delete, op_count: 1,
                mo: Motion { count: 1, mo: MotionType::Word(Direction::Forward) },
                target_register: 'a'
            }
        );
        Ok(())
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


