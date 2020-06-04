
use std::fmt;
use std::collections::HashMap;
use winit::event::{KeyboardInput, VirtualKeyCode, ElementState};
use super::*;

type Event<'a> = winit::event::WindowEvent<'a>;

pub enum CursorStyle {
    Line, Block, Box, Underline
}

pub trait Mode : fmt::Display {
    fn event(&mut self, e: Event, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error>;
    fn cursor_style(&self) -> CursorStyle { CursorStyle::Block }
}

pub struct NormalMode {
    pending_buf: String
}

impl NormalMode {
    pub fn new() -> NormalMode {
        NormalMode { pending_buf: String::new() }
    }
}

impl fmt::Display for NormalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "normal [{}]", self.pending_buf)
    }
}


impl Mode for NormalMode {
    fn event(&mut self, e: Event, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error> {
        match e {
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), .. }, .. } => {
                match vk {
                    VirtualKeyCode::Escape => {
                        self.pending_buf.clear();
                        Ok(None)
                    },
                    _ => Ok(None) 
                }
            },
            Event::ReceivedCharacter(c) if !c.is_control() => {
                use super::command::*;
                self.pending_buf.push(c);
                match Command::parse(&self.pending_buf) {
                    Ok(cmd) => {
                        let res = match cmd.execute(buf, registers) {
                            Ok(r) => r,
                            Err(e) => {
                                self.pending_buf.clear();
                                return Err(e);
                            }
                        };
                        self.pending_buf.clear();
                        match res {
                            None | Some(ModeTag::Normal) => Ok(None),
                            Some(ModeTag::Insert) => Ok(Some(Box::new(InsertMode {
                                tmut: buf.text.insert_mutator(buf.cursor_index)
                            }))),
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
    tmut: piece_table::TableMutator
}


impl fmt::Display for InsertMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "insert")
    }
}

impl Mode for InsertMode {
    fn cursor_style(&self) -> CursorStyle { CursorStyle::Line }

    fn event(&mut self, e: Event, buf: &mut Buffer, _: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error> {
        match e {
            Event::ReceivedCharacter(c) if !c.is_control() => {
                self.tmut.push_char(&mut buf.text, c);
                buf.cursor_index += 1;
                Ok(None)
            },
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Back => {
                        if !self.tmut.pop_char(&mut buf.text) {
                            buf.cursor_index -= 1;
                        }
                        Ok(None)
                    },
                    VirtualKeyCode::Return => {
                        self.tmut.push_char(&mut buf.text, '\n');
                        buf.cursor_index += 1;
                        Ok(None)
                    }
                    VirtualKeyCode::Escape => Ok(Some(Box::new(NormalMode::new()))),
                    _ => Ok(None)
                }
            },
            _ => Ok(None)
        }
    }
}
