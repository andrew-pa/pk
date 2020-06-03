#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

mod piece_table_render;

use runic::*;
use pk_common::*;
use pk_common::piece_table;
use pk_common::command::*;
use piece_table_render::PieceTableRenderer;

use std::fmt;
use std::collections::HashMap;

trait Mode : fmt::Display {
    fn event(&mut self, e: runic::Event, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error>;
    fn cursor_style(&self) -> piece_table_render::CursorStyle { piece_table_render::CursorStyle::Block }
}

struct NormalMode {
    pending_buf: String
}

impl NormalMode {
    fn new() -> NormalMode {
        NormalMode { pending_buf: String::new() }
    }
}

impl fmt::Display for NormalMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "normal [{}]", self.pending_buf)
    }
}


impl Mode for NormalMode {
    fn event(&mut self, e: runic::Event, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error> {
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


struct InsertMode {
    tmut: piece_table::TableMutator
}


impl fmt::Display for InsertMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "insert")
    }
}

impl Mode for InsertMode {
    fn cursor_style(&self) -> piece_table_render::CursorStyle { piece_table_render::CursorStyle::Line }

    fn event(&mut self, e: Event, buf: &mut Buffer, registers: &mut HashMap<char, String>) -> Result<Option<Box<dyn Mode>>, Error> {
        match e {
            Event::ReceivedCharacter(c) if !c.is_control() => {
                self.tmut.push_char(&mut buf.text, c);
                buf.cursor_index += 1;
                Ok(None)
            },
            Event::KeyboardInput { input: KeyboardInput { virtual_keycode: Some(vk), state: ElementState::Pressed, .. }, .. } => {
                match vk {
                    VirtualKeyCode::Back => {
                        self.tmut.pop_char(&mut buf.text);
                        buf.cursor_index -= 1;
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

struct Server {
    name: String,
    socket: nng::Socket
}

struct PkApp {
    fnt: Font, buf: Buffer, registers: HashMap<char, String>,
    txr: PieceTableRenderer,
    mode: Box<dyn Mode>, last_err: Option<Error>
}

impl runic::App for PkApp {
    fn init(rx: &mut RenderContext) -> Self {
        let fnt = rx.new_font("Fira Code", 14.0, FontWeight::Regular, FontStyle::Normal).unwrap();
        let txr = PieceTableRenderer::init(rx, fnt.clone());
        PkApp {
            fnt, txr, buf: Buffer::from_file(&std::path::Path::new("pk-runic-client/src/main.rs")).unwrap(),
            mode: Box::new(NormalMode::new()), last_err: None, registers: HashMap::new()
        }
    }

    fn event(&mut self, e: runic::Event) -> bool {
        if let Event::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, .. }, .. } = e {
            self.last_err = None;
        }
        match e {
            Event::CloseRequested => return true,
            _ => {
                match self.mode.event(e, &mut self.buf, &mut self.registers) {
                    Ok(Some(new_mode)) => { self.mode = new_mode },
                    Ok(None) => {},
                    Err(e) => self.last_err = Some(e)
                };
            }
        }
        false
    }

    fn paint(&mut self, rx: &mut RenderContext) {
        rx.clear(Color::black());
        if let Some(e) = &self.last_err {
            rx.set_color(Color::rgb(0.9, 0.1, 0.0));
            rx.draw_text(Rect::xywh(4.0, rx.bounds().h - 16.0, 1000.0, 1000.0), &format!("error: {}", e), &self.fnt);
        }
        rx.set_color(Color::rgb(0.7, 0.35, 0.0));
        rx.draw_text(Rect::xywh(8.0, 2.0, 1000.0, 100.0),
            &format!("{} col {} {}@last{}-start{}-next{}", self.mode, self.buf.current_column(),
                self.buf.cursor_index, self.buf.last_line_index(self.buf.cursor_index),
                self.buf.current_start_of_line(self.buf.cursor_index), self.buf.next_line_index(self.buf.cursor_index)), &self.fnt);
        self.txr.cursor_index = self.buf.cursor_index;
        self.txr.cursor_style = self.mode.cursor_style();
        self.txr.paint(rx, &self.buf.text, Rect::xywh(8.0, 20.0, rx.bounds().w-8.0, rx.bounds().h-20.0));
    }
}

fn main() {
    runic::start::<PkApp>(WindowOptions::new().with_title("pk"))
}
