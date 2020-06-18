#![allow(dead_code)]

extern crate lazy_static;

mod buffer;
mod motion;
mod command;
mod mode;
mod piece_table_render;
mod line_command;
mod server;
mod editor_state;
mod config;

use runic::*;
use pk_common::*;
use pk_common::piece_table::PieceTable;
use piece_table_render::PieceTableRenderer;
use mode::*;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use editor_state::*;
use config::Config;

use std::error::Error as ErrorTrait;

#[derive(Debug)]
pub enum Error {
    IncompleteCommand,
    InvalidCommand(String),
    UnknownCommand(String),
    ConfigParseError(String, Option<toml::Value>),
    EmptyRegister(char),
    Other(Box<dyn ErrorTrait + 'static>)
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IncompleteCommand => write!(f, "incomplete command"),
            Error::InvalidCommand(cmd) => write!(f, "invalid command: {}", cmd),
            Error::UnknownCommand(cmd) => write!(f, "unknown command: {}", cmd),
            Error::ConfigParseError(cmd, val) => write!(f, "bad configuration: {} (value = {:?})", cmd, val),
            Error::EmptyRegister(c) => write!(f, "nothing in register \"{}", c),
            Error::Other(e) => e.fmt(f)
        }
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

impl Error {
    pub fn from_other<E: ErrorTrait + 'static>(e: E) -> Self {
        Error::Other(Box::new(e))
    }
}


struct PkApp {
    fnt: Font,
    txr: PieceTableRenderer,
    mode: Box<dyn Mode>,
    state: PEditorState,
}

impl runic::App for PkApp {
    fn init(rx: &mut RenderContext) -> Self {
        let (config, errmsg) = std::fs::read_to_string(".client.pk.toml").map_or_else(|e| {
                (Config::default(), if e.kind() != std::io::ErrorKind::NotFound { Some(UserMessage::error(
                            format!("error loading configuration file: {}", e), None)) } else { None })
            }, |v|  v.parse::<toml::Value>().map_err(Error::from_other).and_then(Config::from_toml).map_or_else(|e| {
                (Config::default(), Some(UserMessage::error(
                            format!("error parsing configuration file: {}", e), None)))
            }, |v| (v, None)));
        let mut state = EditorState::with_config(config.clone());
        if let Some(em) = errmsg {
            state.process_usr_msg(em);
        }

        let state = Arc::new(RwLock::new(state));
        if let Some(url) = std::env::args().nth(1) {
            EditorState::connect_to_server(state.clone(), "cmdln".into(), &url);
        }

        for (name, url) in config.autoconnect_servers.iter() {
            EditorState::connect_to_server(state.clone(), name.clone(), url);
        }

        let mut asw = editor_state::AutosyncWorker::new(state.clone());
        std::thread::spawn(move || {
            asw.run();
        });

        let fnt = rx.new_font(&config.font.0, config.font.1,
                              FontWeight::Regular, FontStyle::Normal).unwrap();
        let txr = PieceTableRenderer::init(rx, fnt.clone());
        PkApp {
            mode: Box::new(mode::CommandMode::new(state.clone())),
            fnt, txr, state
        }
    }

    fn event(&mut self, e: runic::Event, event_loop_flow: &mut ControlFlowOpts, should_redraw: &mut bool) {
        if let Event::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, .. }, .. } = e {
            *should_redraw = true;
        }
        if { self.state.read().unwrap().force_redraw } {
            *should_redraw = true;
            self.state.write().unwrap().force_redraw = false;
        }
        match e {
            Event::CloseRequested => *event_loop_flow = ControlFlowOpts::Exit,
            _ => {
                match self.mode.event(e, self.state.clone()) {
                    Ok(Some(new_mode)) => { self.mode = new_mode },
                    Ok(None) => {},
                    Err(e) => {
                        println!("{:?}", e);
                        self.mode = Box::new(NormalMode::new());
                        self.state.write().unwrap().process_error(e);
                    }
                };
            }
        }
    }

    fn paint(&mut self, rx: &mut RenderContext) {
        rx.clear(Color::black());

        let state = self.state.read().unwrap();

        /*let mut x = 0f32;
        for c in state.config.colors.accent.iter() {
            rx.set_color(*c);
            rx.fill_rect(Rect::xywh(x, 256.0, 64.0, 64.0));
            x += 64.0;
        }*/

        if state.usrmsgs.len() > 0 {
            let x = 8f32; let mut y = rx.bounds().h-8f32; 
            for (i, um) in state.usrmsgs.iter().enumerate().rev() {
                rx.set_color(match um.mtype {
                    UserMessageType::Error => state.config.colors.accent[0],
                    UserMessageType::Warning => state.config.colors.accent[2],
                    UserMessageType::Info => state.config.colors.half_gray,
                });
                let msg_tf = rx.new_text_layout(&um.message, &self.fnt, rx.bounds().w, 1000.0).unwrap();
                let msgb = msg_tf.bounds();
                let sy = y;
                y -= msgb.h;
                if let Some((opts, _)) = um.actions.as_ref() {
                    let mut x = x + msgb.w * 0.1;
                    for (j, op) in opts.iter().enumerate() {
                        let f = rx.new_text_layout(&format!("[{}] {}", j+1, op), &self.fnt, 1000.0, 1000.0).unwrap();
                        f.color_range(rx, 0..3, state.config.colors.accent[6]);
                        rx.draw_text_layout(Point::xy(x,y), &f);
                        x += f.bounds().w + self.txr.em_bounds.w*3.0;
                    }
                    y -= msgb.h;
                }
                rx.draw_text_layout(Point::xy(x, y), &msg_tf);
                if self.mode.mode_tag() == ModeTag::UserMessage && state.selected_usrmsg == i {
                    rx.stroke_rect(Rect::xywh(x-2f32, y, rx.bounds().w-12f32, sy-y), 2.0);
                }
            }
        }

        if state.buffers.len() > 0 {
            let buf = &state.buffers[state.current_buffer];

            // draw status line
            rx.set_color(state.config.colors.quarter_gray);
            rx.fill_rect(Rect::xywh(0.0, 0.0, rx.bounds().w, self.txr.em_bounds.h+2.0));
            rx.set_color(state.config.colors.accent[1]);
            rx.draw_text(Rect::xywh(8.0, 2.0, rx.bounds().w, 1000.0),
                &format!("{} / col {} / {}:{} v{}{}", self.mode, buf.current_column(),
                buf.server_name, buf.path.to_str().unwrap_or("!"), buf.version,
                if buf.currently_in_conflict { "⮾" } else { "" }
                ), &self.fnt);

            self.txr.cursor_index = buf.cursor_index;
            self.txr.cursor_style = self.mode.cursor_style();
            self.txr.paint(rx, &buf.text, Rect::xywh(8.0, self.txr.em_bounds.h+2.0, rx.bounds().w, rx.bounds().h-20.0));
        } else {
            rx.set_color(state.config.colors.accent[5]);
            rx.draw_text(Rect::xywh(80.0, rx.bounds().h/4.0, rx.bounds().w, rx.bounds().h-20.0), 
                         "enter a command to begin", &self.fnt);
        }

        if let Some((cmd_cur_index, pending_cmd)) = state.command_line.as_ref() {
            rx.set_color(state.config.colors.quarter_gray);
            rx.fill_rect(Rect::xywh(0.0, self.txr.em_bounds.h+2.0, rx.bounds().w, self.txr.em_bounds.h+2.0));
            rx.set_color(state.config.colors.three_quarter_gray);
            self.txr.cursor_index = *cmd_cur_index;
            self.txr.cursor_style = CursorStyle::Line;
            self.txr.paint(rx, pending_cmd, Rect::xywh(8.0, self.txr.em_bounds.h+2.0, rx.bounds().w-8.0, rx.bounds().h-20.0));
        }

        /*let mut y = 30.0;
        let mut global_index = 0;
        for p in self.buf.text.pieces.iter() {
            rx.draw_text(Rect::xywh(rx.bounds().w / 2.0, y, 1000.0, 1000.0), &format!("{}| \"{}\"", global_index, 
                                                                        &self.buf.text.sources[p.source][p.start..p.start+p.length].escape_debug()), &self.fnt);
            global_index += p.length;
            y += 16.0;
        }*/
    }
}

fn main() {
    runic::start::<PkApp>(WindowOptions::new().with_title("pk"))
}
