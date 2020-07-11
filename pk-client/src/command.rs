
use super::*;
use super::motion::*;
use std::ops::Range;
use std::collections::HashMap;


#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent(Direction),
    MoveAndEnterMode(ModeTag),
    NewLineAndEnterMode(Direction, ModeTag),
    ReplaceChar(char)
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Command {
    Move(Motion),
    Repeat { count: usize },
    Undo { count: usize },
    Redo { count: usize },
    JoinLine { count: usize },
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
    Leader(char),
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
            Some('.') => return Ok(Command::Repeat { count: opcount.unwrap_or(1) }),
            Some('u') => return Ok(Command::Undo { count: opcount.unwrap_or(1) }),
            Some('U') => return Ok(Command::Redo { count: opcount.unwrap_or(1) }),
            Some('J') => return Ok(Command::JoinLine { count: opcount.unwrap_or(1) }),
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
            Some(' ') => { schars.next(); return match schars.next() {
                Some(c) => Ok(Command::Leader(c)),
                None => Err(Error::IncompleteCommand)
            } },
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

    fn count_mut(&mut self) -> Option<&mut usize> {
        match self {
            Command::JoinLine { count } => Some(count),
            Command::Put { count, .. } => Some(count),
            Command::Edit { op_count, .. } => Some(op_count),
            _ => None
        }
    }
     
    pub fn execute(&self, state: &mut editor_state::EditorState, client: PClientState) -> Result<Option<ModeTag>, Error> {
        if let Command::Repeat { count } = self {
            let mut cmd = state.last_command.ok_or_else(|| Error::InvalidCommand("no previous command".into()))?;
            dbg!(cmd);
            if let Command::ChangeMode(ModeTag::Insert) = cmd {
                if let Some(buf) = state.current_buffer_index() {
                    let buf = &mut state.buffers[buf];
                    let inserted_piece = match buf.text.history.last() {
                        Some(act) => {
                            if let piece_table::Change::Insert { new, .. } = act.changes[if act.changes.len() == 1 { 0 } else { 1 }] {
                                new
                            } else {
                                panic!();
                            }
                        },
                        None => panic!()
                    };
                    dbg!(inserted_piece);
                    for _ in 0..*count {
                        buf.text.insert_raw_piece(buf.cursor_index, inserted_piece.clone());
                        buf.cursor_index += inserted_piece.length;
                    }
                }
                return Ok(None);
            }
            else {
                cmd.count_mut().map(|c| *c = *count);
                return cmd.execute(state, client);
            }
        }
        match self {
            Command::Move(mo) => {
                if let Some(buf) = state.current_buffer_mut() {
                    let Range { start: _, end } = mo.range(buf, buf.cursor_index, 1);
                    buf.cursor_index = end;
                }
                Ok(None)
            },
            Command::Put { count, source_register, clear_register } => {
                state.last_command = Some(*self);
                if let Some(buf) = state.current_buffer_index() {
                    let buf = &mut state.buffers[buf];
                    let src = state.registers.get(source_register).ok_or(Error::EmptyRegister(*source_register))?;
                    buf.text.insert_range(src, buf.cursor_index);
                    buf.cursor_index += src.len();
                    if *clear_register {
                        state.registers.remove(source_register);
                    }
                }
                Ok(None)
            },
            Command::Undo { count } => {
                if let Some(buf) = state.current_buffer_mut() {
                    for _ in 0..*count {
                        buf.text.undo();
                    }
                }
                Ok(None)
            },
            Command::JoinLine { count } => {
                state.last_command = Some(*self);
                if let Some(buf) = state.current_buffer_mut() {
                    for _ in 0..*count {
                        let ln = buf.next_line_index(buf.cursor_index);
                        buf.text.delete_range(ln-1, ln);
                    }
                }
                Ok(None)
            },
            Command::Edit { op, op_count, mo, target_register } => {
                state.last_command = Some(*self);
                let buf = if let Some(b) = state.current_buffer_index() { 
                    &mut state.buffers[b]
                } else { return Err(Error::InvalidCommand("".into())); };
                match op {
                    Operator::Delete | Operator::Change => {
                        let mut r = mo.range(buf, buf.cursor_index, *op_count);
                        if let MotionType::An(_) = mo.mo {
                            r.end += 1;
                        }
                        if let MotionType::Inner(_) = mo.mo {
                            r.end += 1;
                        }
                        if r.start != r.end {
                            // adjust range for changing so that it doesn't grab trailing
                            // whitespace, especially newlines
                            if *op == Operator::Change && buf.text.char_at(r.start).map(motion::CharClassify::class)
                                .map_or(false, |c| c != CharClass::Whitespace) {
                                    while buf.text.char_at(r.end.saturating_sub(1)).map(motion::CharClassify::class)
                                        .map_or(false, |c| c == CharClass::Whitespace) {
                                            println!("{}", r.end);
                                            r.end = r.end.saturating_sub(1);
                                        }
                            }
                            state.registers.insert(*target_register, buf.text.copy_range(r.start, r.end));
                            buf.text.delete_range(r.start, r.end);
                        }
                        buf.cursor_index = r.start;
                        Ok(if *op == Operator::Change {
                            Some(ModeTag::Insert)
                        } else {
                            None
                        })
                    },
                    Operator::Yank => {
                        let mut r = mo.range(buf, buf.cursor_index, *op_count);
                        if let MotionType::An(_) = mo.mo {
                            r.end += 1;
                        }
                        if let MotionType::Inner(_) = mo.mo {
                            r.end += 1;
                        }
                        state.registers.insert(*target_register, buf.text.copy_range(r.start, r.end));
                        Ok(None)
                    },
                    Operator::ReplaceChar(c) => {
                        let cursor_index = buf.cursor_index;
                        buf.text.delete_range(cursor_index, cursor_index+1);
                        let mut m = buf.text.insert_mutator(cursor_index);
                        m.push_char(&mut buf.text, *c);
                        Ok(None)
                    },
                    Operator::MoveAndEnterMode(mode) => {
                        let Range { start: _, end } = mo.range(buf, buf.cursor_index, 1);
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
                        if idx == buf.text.len()-1 {
                            buf.cursor_index = 1;
                        }
                        Ok(Some(*mode))
                    }
                    Operator::Indent(direction) => {
                        let r = mo.range(buf, buf.cursor_index, *op_count);
                        let mut ln = buf.current_start_of_line(r.start);
                        while ln <= r.end {
                            if *direction == Direction::Forward {
                                buf.indent(ln, 1, &client.read().unwrap().config);
                            } else {
                                buf.undent(ln, 1, &client.read().unwrap().config);
                            }
                            ln = buf.next_line_index(ln);
                        }
                        Ok(None)
                    }, 
                }
            },

            &Command::ChangeMode(mode) => {
                if mode == ModeTag::Insert {
                    state.last_command = Some(*self);
                }
                Ok(Some(mode))
            },
            Command::Leader(c) => match c {
                'h' | 'j' | 'k' | 'l' => {
                    if let Some(ng) = state.current_pane().neighbors[match c {
                        'h' => 0,
                        'j' => 3,
                        'k' => 2,
                        'l' => 1,
                        _ => unreachable!()
                    }] {
                        state.current_pane = ng;
                    }
                    Ok(None)
                },
                's' | 'v' => {
                    let nc = state.current_pane().content.clone();
                    Pane::split(&mut state.panes, state.current_pane, *c == 's', 0.5, nc);
                    Ok(None)
                },
                'x' => {
                    if state.panes.len() == 1 {
                        return Err(Error::InvalidCommand("can't delete all panes".into()));
                    }
                    state.current_pane = Pane::remove(&mut state.panes, state.current_pane);
                    Ok(None)
                },
                _ => Err(Error::UnknownCommand(format!("unknown leader command {}", c)))
            },

            _ => Err(Error::UnknownCommand(format!("unimplemented {:?}", self)))
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
