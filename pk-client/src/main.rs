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
mod syntax_highlight;

use runic::*;
use pk_common::*;
use pk_common::piece_table::PieceTable;
use piece_table_render::PieceTableRenderer;use mode::*;use std::rc::Rc;
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
    cmd_txr: PieceTableRenderer,
    mode: Box<dyn Mode>,
    state: PEditorState,
    client: PClientState,
    synh: Option<Vec<piece_table_render::Highlight>>,
    last_highlighted_version: usize,
    highlighter: syntax_highlight::Highlighter
}

impl runic::App for PkApp {
    fn init(rx: &mut RenderContext) -> Self {
        let mut cargs = pico_args::Arguments::from_env();

        let projd = directories_next::ProjectDirs::from("", "", "pk").expect("compute application directory");
        let config_dir = cargs.opt_value_from_str("--config").unwrap()
            .unwrap_or_else(|| std::path::Path::join(projd.config_dir(), "client.toml"));
        let (config, errmsg) = std::fs::read_to_string(config_dir).map_or_else(|e| {
                (Config::default(), if e.kind() != std::io::ErrorKind::NotFound { Some(UserMessage::error(
                            format!("error loading configuration file: {}", e), None)) } else { None })
            }, |v|  v.parse::<toml::Value>().map_err(Error::from_other).and_then(Config::from_toml).map_or_else(|e| {
                (Config::default(), Some(UserMessage::error(
                            format!("error parsing configuration file: {}", e), None)))
            }, |v| (v, None)));

        let mut client = ClientState::with_config(config.clone());
        let mut estate = EditorState::new();
        if let Some(em) = errmsg {
            client.process_usr_msg(em);
        }

        estate.panes.insert(0, Pane::whole_screen(PaneContent::Empty));

        let client = Arc::new(RwLock::new(client));
        let estate = Arc::new(RwLock::new(estate));
        if let Some(url) = cargs.opt_value_from_str::<&str, String>("--server").unwrap() {
            ClientState::connect_to_server(client.clone(), "cmdln".into(), &url);
        }

        for (name, url) in config.autoconnect_servers.iter() {
            ClientState::connect_to_server(client.clone(), name.clone(), url);
        }

        let free_args = cargs.free().unwrap();
        for farg in free_args.iter() {
            ClientState::open_buffer(client.clone(), estate.clone(), "local".into(), std::path::PathBuf::from(farg),
            |estate, _, buffer_index| {
                let cnt = PaneContent::buffer(buffer_index);
                if estate.panes.len() == 1 {
                    estate.current_pane_mut().content = cnt;
                } else {
                    Pane::split(&mut estate.panes, 0, true, 0.5, cnt);
                }
            });
        }

        let mut asw = editor_state::AutosyncWorker::new(client.clone(), estate.clone());
        std::thread::spawn(move || {
            asw.run();
        });

        let highlighter = syntax_highlight::Highlighter::from_toml(config.syntax_coloring.as_ref());

        let fnt = rx.new_font(&config.font.0, config.font.1,
                              FontWeight::Regular, FontStyle::Normal).unwrap();
        let em_bounds = rx.new_text_layout("M", &fnt, 100.0, 100.0).expect("create em size layout").bounds();
        let txr = PieceTableRenderer::init(rx, fnt.clone(), em_bounds);
        let mut cmd_txr = PieceTableRenderer::init(rx, fnt.clone(), em_bounds);
        cmd_txr.cursor_style = CursorStyle::Line;
        cmd_txr.highlight_line = false;
        PkApp {
            mode: if free_args.len() == 0 { Box::new(mode::CommandMode::new()) } else { Box::new(mode::NormalMode::new()) },
            fnt, txr, cmd_txr, state: estate, client, synh: None, last_highlighted_version: 0,
            highlighter
        }
    }

    fn event(&mut self, e: runic::Event, event_loop_flow: &mut ControlFlowOpts, should_redraw: &mut bool) {
        if let Event::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, .. }, .. } = e {
            *should_redraw = true;
        }
        if self.client.read().unwrap().force_redraw {
            *should_redraw = true;
            self.client.write().unwrap().force_redraw = false;
        }

        match e {
            Event::CloseRequested => *event_loop_flow = ControlFlowOpts::Exit,
            _ => {
                match self.mode.event(e, self.client.clone(), self.state.clone()) {
                    Ok(Some(new_mode)) => { self.mode = new_mode },
                    Ok(None) => {},
                    Err(e) => {
                        println!("{:?}", e);
                        self.mode = Box::new(NormalMode::new());
                        self.client.write().unwrap().process_error(e);
                    }
                };
            }
        }
    }

    fn paint(&mut self, rx: &mut RenderContext) {
        let start = std::time::Instant::now();
        let client = self.client.read().unwrap();
        let mut state = self.state.write().unwrap();

        let config = &client.config;

        rx.clear(config.colors.background);

        // might be nice to expose this as some sort of command for debugging color schemes
        // let mut x = 0f32;
        // for c in state.config.colors.accent.iter() {
        //     rx.set_color(*c);
        //     rx.fill_rect(Rect::xywh(x, 256.0, 64.0, 64.0));
        //     x += 64.0;
        // }

        let usrmsg_y = if client.usrmsgs.len() > 0 {
            let x = 8f32; let mut y = rx.bounds().h-8f32; 
            for (i, um) in client.usrmsgs.iter().enumerate().rev() {
                rx.set_color(match um.mtype {
                    UserMessageType::Error => config.colors.accent[0],
                    UserMessageType::Warning => config.colors.accent[2],
                    UserMessageType::Info => config.colors.half_gray,
                });
                let msg_tf = rx.new_text_layout(&um.message, &self.fnt, rx.bounds().w, 1000.0).unwrap();
                let msgb = msg_tf.bounds();
                let sy = y;
                y -= msgb.h;
                if let Some((opts, _)) = um.actions.as_ref() {
                    let mut x = x + msgb.w * 0.1;
                    for (j, op) in opts.iter().enumerate() {
                        let f = rx.new_text_layout(&format!("[{}] {}", j+1, op), &self.fnt, 1000.0, 1000.0).unwrap();
                        f.color_range(rx, 0..3, config.colors.accent[6]);
                        rx.draw_text_layout(Point::xy(x,y), &f);
                        x += f.bounds().w + self.txr.em_bounds.w*3.0;
                    }
                    y -= msgb.h;
                }
                rx.draw_text_layout(Point::xy(x, y), &msg_tf);
                if self.mode.mode_tag() == ModeTag::UserMessage && client.selected_usrmsg == i {
                    rx.stroke_rect(Rect::xywh(x-2f32, y-1.0, rx.bounds().w-12f32, sy-y + 1.0), 2.0);
                }
            }
            y -= 8.0;
            rx.fill_rect(Rect::xywh(0.0, y, rx.bounds().w, 1.0));
            y
        } else { rx.bounds().h };

        let screen_bounds = Rect::xywh(0.0, 0.0, rx.bounds().w, usrmsg_y);

        for i in state.panes.keys().cloned().collect::<Vec<_>>() {
            let bounds = Rect::xywh(screen_bounds.x + screen_bounds.w * state.panes[&i].bounds.x + 1.0,
                                    screen_bounds.y + screen_bounds.h * state.panes[&i].bounds.y + 1.0,
                                    screen_bounds.w * state.panes[&i].bounds.w - 1.0, screen_bounds.h * state.panes[&i].bounds.h - 1.0);

            let active = i == state.current_pane;

            rx.set_color(if active { config.colors.half_gray } else { config.colors.quarter_gray });
            rx.stroke_rect(bounds, 1.0);

            match state.panes[&i].content {
                PaneContent::Buffer { buffer_index, viewport_start, .. } => {
                    let buf = &mut state.buffers[buffer_index];
                    let editor_bounds = Rect::xywh(bounds.x, bounds.y + self.txr.em_bounds.h + 4.0, bounds.w,
                                                       bounds.h);
                    let curln = buf.line_for_index(buf.cursor_index);

                    // draw status line
                    rx.set_color(config.colors.quarter_gray);
                    rx.fill_rect(Rect::xywh(bounds.x, bounds.y, bounds.w, self.txr.em_bounds.h+2.0));
                    rx.set_color(if active { config.colors.accent[1] } else { config.colors.three_quarter_gray });
                    rx.draw_text(Rect::xywh(bounds.x + 8.0, bounds.y + 1.0, bounds.w, 1000.0),
                        &format!("{} | ln {} col {} | {}:{} v{}{} [{}]", self.mode, curln + 1,
                            buf.column_for_index(buf.cursor_index),
                            buf.server_name, buf.path.to_str().unwrap_or("!"), buf.version,
                            if buf.currently_in_conflict { "⮾" } else { "" }, buf.format.stype
                    ), &self.fnt);

                    self.txr.cursor_style = if active { self.mode.cursor_style() } else { CursorStyle::Box };
                    let mut vp = viewport_start;
                    self.txr.ensure_line_visible(&mut vp, curln, editor_bounds);
                    if buf.highlights.is_none() || buf.last_highlighted_action_id < buf.text.most_recent_action_id() 
                        || self.mode.mode_tag() == ModeTag::Insert
                    {
                        //let hstart = std::time::Instant::now();
                        buf.highlights = Some(self.highlighter.compute_highlighting(buf));
                        buf.last_highlighted_action_id = buf.text.most_recent_action_id();
                        self.txr.invalidate_layout_cashe(buf.current_start_of_line(buf.cursor_index) .. buf.next_line_index(buf.cursor_index));
                        // println!("highlight took {}ms", (std::time::Instant::now()-hstart).as_nanos() as f32 / 1000000.0);
                    }
                    self.txr.paint(rx, &buf.text, vp, buf.cursor_index,
                        &config, editor_bounds, buf.highlights.as_ref(), true, self.mode.selection());

                     /*let mut y = 30.0;
                     let mut global_index = 0;
                     for p in buf.text.pieces.iter() {
                         rx.draw_text(Rect::xywh(rx.bounds().w / 2.0, y, 1000.0, 1000.0),
                             &format!("{}| \"{}\"", global_index, 
                                 &buf.text.sources[p.source][p.start..p.start+p.length].escape_debug()), &self.fnt);
                         global_index += p.length;
                         y += 16.0;
                     }*/
                    state.panes.get_mut(&i).unwrap().content = PaneContent::Buffer {
                        buffer_index,
                        viewport_start: vp,
                        viewport_end: self.txr.viewport_end(vp, &editor_bounds)
                    };
                },
                PaneContent::Empty => {
                    rx.set_color(config.colors.accent[5]);
                    rx.draw_text(bounds.offset(Point::xy(self.txr.em_bounds.w, self.txr.em_bounds.h*3.0)), 
                        "enter a command to begin", &self.fnt);
                }
            }
        }


        if let Some((cmd_cur_index, pending_cmd)) = self.mode.cmd_line() {
            rx.set_color(config.colors.quarter_gray);
            rx.fill_rect(Rect::xywh(0.0, self.txr.em_bounds.h+2.0, rx.bounds().w, self.txr.em_bounds.h+2.0));
            rx.set_color(config.colors.three_quarter_gray);
            self.cmd_txr.paint(rx, pending_cmd, 0, cmd_cur_index, &config,
                               Rect::xywh(8.0, self.txr.em_bounds.h+2.0, rx.bounds().w-8.0, rx.bounds().h-20.0),
                               None, false, None);
        }

        let end = std::time::Instant::now();
        rx.set_color(config.colors.quarter_gray);
        rx.draw_text(Rect::xywh(rx.bounds().w-148.0, rx.bounds().h - 20.0, 1000.0, 1000.0), &format!("f{}ms", (end-start).as_nanos() as f32 / 1000000.0), &self.fnt);

    }
}

fn main() {
    runic::start::<PkApp>(WindowOptions::new().with_title("pk"))
}
