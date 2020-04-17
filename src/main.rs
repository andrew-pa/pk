extern crate runic;

mod piece_table;
use runic::*;
use piece_table::{PieceTable, PieceTableRenderer};
use std::error::Error as ErrorTrait;

#[derive(Debug)]
enum Error {
    Other(Box<dyn ErrorTrait + 'static>)
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error")
    }
}

impl ErrorTrait for Error {
    fn source(&self)  -> Option<&(dyn ErrorTrait + 'static)> {
        match self {
            Error::Other(e) => Some(e.as_ref()),
            _ => None
        }
    }
}


enum Operator {
    Move,
    Repeat,
    Undo,
    Delete,
    Change,
    Yank,
    Put
}

enum Direction { Forward, Backward }

enum TextObject {
    Char,
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
    Line,
    StartOfLine,
    EndOfLine,
    Paragraph
}

enum TextObjectMod {
    None, AnObject, InnerObject
}

struct Motion {
    count: usize,
    object: TextObject,
    modifier: TextObjectMod
}

enum Command {
    Edit {
        op: Operator,
        op_count: usize,
        mo: Motion,
    },
    ChangeMode { new_mode: Box<dyn Mode> },
}

impl Command {
    fn parse(s: &str) -> Result<Option<Command>, Error> {
        Ok(None)
    }

    fn execute(&self, buf: &mut Buffer) -> Result<Option<Box<dyn Mode>>, Error> {
        Ok(None)
    }
}

trait Mode {
    fn status_tag(&self) -> &'static str;
    fn event(&mut self, e: runic::Event, buf: &mut Buffer) -> Result<Option<Box<dyn Mode>>, Error>;
}

struct NormalMode {
    pending_buf: String
}

impl NormalMode {
    fn new() -> NormalMode {
        NormalMode { pending_buf: String::new() }
    }
}

impl Mode for NormalMode {
    fn status_tag(&self) -> &'static str { "normal" }
    fn event(&mut self, e: runic::Event, buf: &mut Buffer) -> Result<Option<Box<dyn Mode>>, Error> {
        match e {
            Event::ReceivedCharacter(c) => {
                self.pending_buf.push(c);
                match Command::parse(&self.pending_buf) {
                    Ok(Some(cmd)) => {
                        let res = cmd.execute(buf);
                        self.pending_buf.clear();
                        res
                    },
                    Ok(None) => Ok(None),
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

impl Mode for InsertMode {
    fn status_tag(&self) -> &'static str { "insert" }
    fn event(&mut self, e: Event, buf: &mut Buffer) -> Result<Option<Box<dyn Mode>>, Error> {
        match e {
            Event::ReceivedCharacter(c) => {
                self.tmut.push_char(&mut buf.text, c);
                buf.cursor_index += 1;
                Ok(None)
            },
            Event::KeyboardInput { input, .. } => {
                match input.virtual_keycode {
                    Some(VirtualKeyCode::Back) => {
                        self.tmut.pop_char(&mut buf.text);
                        buf.cursor_index -= 1;
                        Ok(None)
                    },
                    Some(VirtualKeyCode::Escape) => Ok(Some(Box::new(NormalMode::new()))),
                    _ => Ok(None)
                }
            },
            _ => Ok(None)
        }
    }
}

struct Buffer {
    text: PieceTable,
    path: Option<std::path::PathBuf>,
    cursor_index: usize
}

impl Default for Buffer {
    fn default() -> Buffer {
        Buffer {
            text: PieceTable::default(),
            path: None,
            cursor_index: 0
        }
    }
}

struct PkApp {
    fnt: Font, buf: Buffer, txr: PieceTableRenderer,
    mode: Box<dyn Mode>
}

impl runic::App for PkApp {
    fn init(rx: &mut RenderContext) -> Self {
        let fnt = rx.new_font("Fira Code", 12.0, FontWeight::Regular, FontStyle::Normal).unwrap();
        let txr = PieceTableRenderer::init(rx, fnt.clone());
        PkApp {
            fnt, txr, buf: Buffer::default(),
            mode: Box::new(NormalMode::new())
        }
    }

    fn event(&mut self, e: runic::Event) -> bool {
        match e {
            Event::CloseRequested => return true,
            _ => {
                if let Some(new_mode) = self.mode.event(e, &mut self.buf).unwrap() {
                    self.mode = new_mode;
                }
            }
        }
        false
    }

    fn paint(&mut self, rx: &mut RenderContext) {
        rx.clear(Color::black());
        rx.set_color(Color::rgb(0.7, 0.35, 0.0));
        rx.draw_text(Rect::xywh(4.0, 2.0, 100.0, 100.0),  self.mode.status_tag(), &self.fnt);
        self.txr.cursor_index = self.buf.cursor_index;
        self.txr.paint(rx, &self.buf.text, Point::xy(8.0, 18.0));
    }
}

fn main() {
    runic::start::<PkApp>(WindowOptions::new().with_title("pk"))
}
