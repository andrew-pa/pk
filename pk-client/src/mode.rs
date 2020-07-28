
use std::fmt;
use runic::*;
use super::*;
use std::ops::Range;

pub enum CursorStyle {
    Line, Block, Box, Underline
}

pub type ModeEventResult = Result<Option<Box<dyn Mode>>, Error>;

pub trait Mode : fmt::Display {
    fn event(&mut self, e: Event, client: PClientState, state: PEditorState) -> ModeEventResult;
    fn mode_tag(&self) -> ModeTag;
    fn cursor_style(&self) -> CursorStyle { CursorStyle::Block }
    fn cmd_line(&self) -> Option<(usize, &PieceTable)> { None }
    fn selection(&self) -> Option<&Range<usize>> { None }
}

pub struct NormalMode {
    pending_buf: String, ctrl_pressed: bool
}

impl NormalMode {
    pub fn new() -> NormalMode {
        NormalMode { pending_buf: String::new(), ctrl_pressed: false }
    }
}

impl fmt::Display for NormalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "normal [{}]", self.pending_buf)
    }
}

impl Mode for NormalMode {
    fn mode_tag(&self) -> ModeTag {
        ModeTag::Normal
    }

    fn event(&mut self, e: Event, client: PClientState, state: PEditorState) -> ModeEventResult {
        match e {
            Event::ModifiersChanged(ms) => {
                self.ctrl_pressed = ms.ctrl();
                Ok(None)
            }
            
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Escape => {
                        self.pending_buf.clear();
                        Ok(None)
                    },
                    VirtualKeyCode::Left => {
                        let mut state = state.write().unwrap();
                        match &mut state.current_pane_mut().content {
                            PaneContent::Buffer { buffer_index, .. } => 
                                *buffer_index = buffer_index.saturating_sub(1),
                            _ => {}
                        }
                        Ok(None)
                    },
                    VirtualKeyCode::Right => {
                        let mut state = state.write().unwrap();
                        let numbufs = state.buffers.len();
                        match &mut state.current_pane_mut().content {
                            PaneContent::Buffer { buffer_index, .. } => 
                                *buffer_index = (*buffer_index + 1).min(numbufs.saturating_sub(1)),
                            _ => {}
                        }
                        Ok(None)
                    }
                    VirtualKeyCode::E if self.ctrl_pressed => {
                        Ok(Some(Box::new(UserMessageInteractionMode::new(client))))
                    }
                    _ => Ok(None) 
                }
            },
            
            Event::ReceivedCharacter(c) if !c.is_control() => {
                use super::command::*;
                self.pending_buf.push(c);
                match Command::parse(&self.pending_buf) {
                    Ok(cmd) => {
                        let res = {
                            match cmd.execute(&mut state.write().unwrap(), client) {
                                Ok(r) => r,
                                Err(e) => {
                                    self.pending_buf.clear();
                                    return Err(e);
                                }
                            } 
                        };
                        self.pending_buf.clear();
                        match res {
                            None | Some(ModeTag::Normal) => Ok(None),
                            Some(ModeTag::Command) => Ok(Some(Box::new(CommandMode::new()))),
                            Some(ModeTag::Search(dir)) => Ok(Some(Box::new(CommandMode::search(dir)))),
                            Some(ModeTag::Visual) => Ok(Some(Box::new(VisualMode::new(state.read().unwrap().current_buffer().unwrap().cursor_index)))),
                            Some(ModeTag::Insert) => {
                                let mut state = state.write().unwrap();
                                if let PaneContent::Buffer { buffer_index, .. } = state.current_pane().content {
                                    let buf = &mut state.buffers[buffer_index];
                                    Ok(Some(Box::new(InsertMode::new(buf.text.insert_mutator(buf.cursor_index))))) 
                                } else {
                                    Err(Error::InvalidCommand("".into()))
                                }
                            },
                            _ => panic!("unknown mode: {:?}", res)
                        }
                    },
                    Err(Error::IncompleteCommand) => Ok(None),
                    Err(e) => { 
                        self.pending_buf.clear();
                        Err(e)
                    }
                }
                /*Ok(Some(Box::new(InsertMode {
                  tmut: buf.text.insert_mutator(buf.cursor_index)
                  })))*/
            },
            _ => Ok(None)
        }
    }
}


pub struct InsertMode {
    tmut: Option<piece_table::TableMutator>,
    shift_pressed: bool
}

impl InsertMode {
    fn new(tmut: piece_table::TableMutator) -> InsertMode {
        InsertMode {
            tmut: Some(tmut), shift_pressed: false
        }
    }
}

impl fmt::Display for InsertMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "insert")
    }
}

impl Mode for InsertMode {
    fn cursor_style(&self) -> CursorStyle { CursorStyle::Line }

    fn mode_tag(&self) -> ModeTag {
        ModeTag::Insert
    }

    fn event(&mut self, e: Event, client: PClientState, state: PEditorState) -> ModeEventResult {
        let mut state = state.write().unwrap();
        if let PaneContent::Buffer { buffer_index, .. } = state.current_pane().content {
            let buf = &mut state.buffers[buffer_index];
            match e {
                Event::ReceivedCharacter(c) if !c.is_control() => {
                    self.tmut.as_mut().unwrap().push_char(&mut buf.text, c);
                    buf.cursor_index += 1;
                    Ok(None)
                },
                Event::ModifiersChanged(ms) => {
                    self.shift_pressed = ms.shift();
                    Ok(None)
                },
                Event::KeyboardInput {
                    input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, ..
                } => {
                    match vk {
                        VirtualKeyCode::Tab => {
                            let (softtab, tabstop) = {
                                let cfg = &client.read().unwrap().config;
                                (cfg.softtab, cfg.tabstop)
                            };

                            if softtab || !self.shift_pressed {
                                for _ in 0..tabstop {
                                    self.tmut.as_mut().unwrap().push_char(&mut buf.text, ' ');
                                }
                                buf.cursor_index += tabstop;
                            } else {
                                self.tmut.as_mut().unwrap().push_char(&mut buf.text, '\t');
                                buf.cursor_index += 1;
                            }
                            Ok(None)
                        },
                        VirtualKeyCode::Back => {
                            if !self.tmut.as_mut().unwrap().pop_char(&mut buf.text) {
                                buf.cursor_index -= 1;
                            }
                            Ok(None)
                        },
                        VirtualKeyCode::Return => {
                            self.tmut.as_mut().unwrap().push_char(&mut buf.text, '\n');
                            let cfg = &client.read().unwrap().config;
                            buf.cursor_index += 1 + buf.indent_with_mutator(self.tmut.as_mut().unwrap(), buf.sense_indent_level(buf.cursor_index, cfg), cfg);
                            Ok(None)
                        }
                        VirtualKeyCode::Escape => {
                            self.tmut.take().unwrap().finish(&mut buf.text);
                            Ok(Some(Box::new(NormalMode::new())))
                        },
                        _ => Ok(None)
                    }
                },
                _ => Ok(None)
            }
        } else { panic!(); }
    }
}

pub struct VisualMode {
    pending_buf: String,
    selection: Range<usize>,
    side: bool
}

impl VisualMode {
    fn new(start_index: usize) -> VisualMode {
        VisualMode {
            pending_buf: String::new(),
            selection: start_index .. start_index,
            side: true
        }
   }
}

impl fmt::Display for VisualMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "visual [{}]", self.pending_buf)
    }
}
    
impl Mode for VisualMode {
    fn cursor_style(&self) -> CursorStyle { CursorStyle::Block }
    
    fn mode_tag(&self) -> ModeTag {
        ModeTag::Visual
    }
    
    fn selection(&self) -> Option<&Range<usize>> {
        Some(&self.selection)
    }
    
    fn event(&mut self, e: Event, client: PClientState, state: PEditorState) -> ModeEventResult {
        match e {
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Escape => {
                        self.pending_buf.clear();
                        Ok(Some(Box::new(NormalMode::new())))
                    },
                    _ => Ok(None) 
                }
            },
            
            Event::ReceivedCharacter(c) if !c.is_control() => {
                use super::command::*;
                self.pending_buf.push(c);
                match Command::parse_2(&self.pending_buf, Some(crate::motion::Motion::passthrough(&self.selection))) {
                    Ok(Command::VisualSwitchSides) => {
                        self.side = !self.side;
                        self.pending_buf.clear();
                        if let Some(buf) = state.write().unwrap().current_buffer_mut() {
                            buf.cursor_index = if self.side {
                                self.selection.end
                            } else {
                                self.selection.start
                            };
                        }
                        Ok(None)
                    },
                    Ok(Command::Move(mo)) => {
                        if let Some(buf) = state.write().unwrap().current_buffer_mut() {
                            let Range { start: _, end } = mo.range(buf, buf.cursor_index, 1);
                            if self.side {
                                self.selection.end = end;
                            } else {
                                self.selection.start = end;
                            }
                            buf.cursor_index = end;
                        }
                        self.pending_buf.clear();
                        Ok(None)
                    },
                    Ok(cmd) => {
                        let res = {
                            match cmd.execute(&mut state.write().unwrap(), client) {
                                Ok(r) => r,
                                Err(e) => {
                                    self.pending_buf.clear();
                                    return Err(e);
                                }
                            } 
                        };
                        self.pending_buf.clear();
                        match res {
                            Some(ModeTag::Visual) => Ok(None),
                            Some(ModeTag::Command) => Ok(Some(Box::new(CommandMode::new()))),
                            None | Some(ModeTag::Normal) => Ok(Some(Box::new(NormalMode::new()))),
                            Some(ModeTag::Insert) => {
                                let mut state = state.write().unwrap();
                                if let PaneContent::Buffer { buffer_index, .. } = state.current_pane().content {
                                    let buf = &mut state.buffers[buffer_index];
                                    Ok(Some(Box::new(InsertMode::new(buf.text.insert_mutator(buf.cursor_index))))) 
                                } else {
                                    Err(Error::InvalidCommand("".into()))
                                }
                            },
                            _ => panic!("unknown mode: {:?}", res)
                        }
                    },
                    Err(Error::IncompleteCommand) => Ok(None),
                    Err(e) => { 
                        self.pending_buf.clear();
                        Err(e)
                    }
                }
                /*Ok(Some(Box::new(InsertMode {
                  tmut: buf.text.insert_mutator(buf.cursor_index)
                  })))*/
            },
            _ => Ok(None)
        }
        }
}

use piece_table::TableMutator;

pub struct CommandMode {
    cursor_mutator: TableMutator,
    commands: Vec<(regex::Regex, Rc<dyn line_command::CommandFn>)>,
    cursor_index: usize,
    command_line: PieceTable
}

impl CommandMode {
    fn with_table(mut pt: PieceTable) -> CommandMode {
        use regex::Regex;
        use line_command::*;
        let len = pt.len();
        let cursor_mutator = pt.insert_mutator(len);
        CommandMode {
            cursor_mutator,
            command_line: pt,
            cursor_index: len,
            commands: vec![
                (Regex::new("^test (.*)").unwrap(), Rc::new(TestCommand)),
                (Regex::new("^dbg pt").unwrap(), Rc::new(DebugPieceTableCommand)),
                (Regex::new("^dbg rg").unwrap(), Rc::new(DebugRegistersCommand)),
                (Regex::new(r#"^e\s+(?:(?P<server_name>\w+):)?(?P<path>.*)"#).unwrap(), Rc::new(EditFileCommand)),
                (Regex::new(r#"^b(?P<subcmd>\w+)?\s+(?P<name_query>.*)"#).unwrap(), Rc::new(BufferCommand)),
                (Regex::new("^sync").unwrap(), Rc::new(SyncFileCommand)),
                (Regex::new(r#"^con\s+(?P<server_name>\w+)\s(?P<server_url>.*)"#).unwrap(), Rc::new(ConnectToServerCommand)),
                (Regex::new(r#"(\?|/)(.*)"#).unwrap(), Rc::new(SearchCommand))
            ],
        }
    }
    
    pub fn new() -> CommandMode {
        CommandMode::with_table(PieceTable::default())
    }
    
    pub fn search(dir: Direction) -> CommandMode {
        CommandMode::with_table(PieceTable::with_text(match dir {
            Direction::Forward => "/",
            Direction::Backward => "?"
        }))
    }
}

impl Mode for CommandMode {
    fn mode_tag(&self) -> ModeTag {
        ModeTag::Command
    }

    fn cursor_style(&self) -> CursorStyle {
        CursorStyle::Box
    }

    fn cmd_line(&self) -> Option<(usize, &PieceTable)> {
        Some((self.cursor_index, &self.command_line))
    }
    
    fn event(&mut self, e: Event, cs: PClientState, es: PEditorState) 
        -> ModeEventResult
    {
        let pending_command = &mut self.command_line;
        match e {
            Event::ReceivedCharacter(c) if !c.is_control() => {
                self.cursor_mutator.push_char(pending_command, c);
                self.cursor_index += 1;
                Ok(None)
            },
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Left => {
                        self.cursor_index = self.cursor_index.saturating_sub(1);
                        self.cursor_mutator = pending_command.insert_mutator(self.cursor_index);
                        Ok(None)
                    },
                    VirtualKeyCode::Right => {
                        self.cursor_index = (self.cursor_index+1).max(pending_command.len());
                        self.cursor_mutator = pending_command.insert_mutator(self.cursor_index);
                        Ok(None)
                    },
                    VirtualKeyCode::Back => {
                        self.cursor_mutator.pop_char(pending_command);
                        self.cursor_index -= 1;
                        Ok(None)
                    },
                    VirtualKeyCode::Return => {
                        let cmdstr = self.command_line.text();
                        if let Some((cmdix, args)) = self.commands.iter().enumerate()
                            .filter_map(|(i,cmd)| cmd.0.captures(&cmdstr).map(|c| (i, c))).nth(0)
                        {
                            let cmd = self.commands[cmdix].1.clone();
                            cmd.process(cs, es, &args)
                        } else {
                            Err(Error::InvalidCommand(cmdstr))
                        }
                    }
                    VirtualKeyCode::Escape => {
                        Ok(Some(Box::new(NormalMode::new())))
                    },
                    _ => Ok(None)
                }
            },
            _ => Ok(None)
        }
    }
}

impl fmt::Display for CommandMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "command")
    }
}

pub struct UserMessageInteractionMode;

impl UserMessageInteractionMode {
    fn new(state: PClientState) -> UserMessageInteractionMode {
        let mut s = state.write().unwrap();
        s.selected_usrmsg = s.usrmsgs.len()-1;
        UserMessageInteractionMode
    }
}

impl Mode for UserMessageInteractionMode {
    fn mode_tag(&self) -> ModeTag {
        ModeTag::UserMessage
    }

    fn cursor_style(&self) -> CursorStyle {
        CursorStyle::Box
    }

    fn event(&mut self, e: Event, state: PClientState, _: PEditorState) -> ModeEventResult {
        match e {
            Event::ReceivedCharacter(c) if c.is_digit(10) => {
                let sel = c.to_digit(10).unwrap() as usize;
                if sel < 1 || sel > 9 { return Ok(None); }
                let um: UserMessage = { let mut s = state.write().unwrap();
                    let sm = s.selected_usrmsg; 
                    s.selected_usrmsg = (sm + 1).min(s.usrmsgs.len().saturating_sub(1));
                    s.usrmsgs.remove(sm)
                };
                if let Some((_, f)) = um.actions {
                    f(sel, state);
                }
                Ok(Some(Box::new(NormalMode::new())))
            },
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Escape => {
                        Ok(Some(Box::new(NormalMode::new())))
                    },
                    VirtualKeyCode::Return | VirtualKeyCode::Back | VirtualKeyCode::Delete => {
                        let mut s = state.write().unwrap();
                        if s.usrmsgs.len() == 0 {
                            return Ok(Some(Box::new(NormalMode::new())));
                        }
                        let sm = s.selected_usrmsg; 
                        s.usrmsgs.remove(sm);
                        s.selected_usrmsg = sm.saturating_sub(1);
                        if s.usrmsgs.len() == 0 {
                            Ok(Some(Box::new(NormalMode::new())))
                        } else {
                            Ok(None)
                        }
                    },
                    VirtualKeyCode::E => {
                        state.write().unwrap().usrmsgs.clear();
                        Ok(Some(Box::new(NormalMode::new())))
                    },
                    VirtualKeyCode::J => {
                        let mut s = state.write().unwrap();
                        s.selected_usrmsg = (s.selected_usrmsg + 1).min(s.usrmsgs.len().saturating_sub(1));
                        Ok(None)
                    },
                    VirtualKeyCode::K => {
                        let mut s = state.write().unwrap();
                        s.selected_usrmsg = s.selected_usrmsg.saturating_sub(1);
                        Ok(None)
                    }
                    _ => Ok(None)
                }
            }
            _ => Ok(None)
        }
    }
}

impl fmt::Display for UserMessageInteractionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "message")
    }
}

