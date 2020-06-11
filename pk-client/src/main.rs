#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

mod motion;
mod command;
mod mode;
mod piece_table_render;
mod line_command;
mod server;
mod editor_state;

use runic::*;
use pk_common::*;
use pk_common::piece_table::PieceTable;
use command::*;
use piece_table_render::PieceTableRenderer;
use std::collections::HashMap;
use mode::*;
use std::rc::Rc;
use server::Server;
use futures::prelude::*;
use std::sync::{Arc, RwLock};
use editor_state::*;

struct PkApp {
    fnt: Font,
    txr: PieceTableRenderer,
    mode: Box<dyn Mode>,
    last_err: Option<Error>,
    state: PEditorState,
}

impl runic::App for PkApp {
    fn init(rx: &mut RenderContext) -> Self {
        let mut state = EditorState::default();
        let srv_url = if let Some(url) = std::env::args().nth(1) {
            url
        } else {
            "ipc://pk".into()
        };
        // instead of panicking, we should probably show a little dialog to the user
        // and ask if they want to connect to a different URL or try to reconnect
        state.connect_to_server("local".into(), &srv_url).expect("connect to local server");

        let fnt = rx.new_font("Fira Code", 14.0, FontWeight::Regular, FontStyle::Normal).unwrap();
        let txr = PieceTableRenderer::init(rx, fnt.clone());
        PkApp {
            fnt, txr, state: Arc::new(RwLock::new(state)),
            mode: Box::new(NormalMode::new()), last_err: None
        }
    }

    fn event(&mut self, e: runic::Event, event_loop_flow: &mut ControlFlowOpts, should_redraw: &mut bool) {
        if let Event::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, .. }, .. } = e {
            self.last_err = None;
            *should_redraw = true;
        }
        match e {
            Event::CloseRequested => *event_loop_flow = ControlFlowOpts::Exit,
            _ => {
                match self.mode.event(e, self.state.clone()) {
                    Ok(Some(new_mode)) => { self.mode = new_mode },
                    Ok(None) => {},
                    Err(e) => self.last_err = Some(e)
                };
            }
        }
    }

    fn paint(&mut self, rx: &mut RenderContext) {
        rx.clear(Color::black());

        if let Some(e) = &self.last_err {
            rx.set_color(Color::rgb(0.9, 0.1, 0.0));
            rx.draw_text(Rect::xywh(4.0, rx.bounds().h - 16.0, 1000.0, 1000.0), &format!("error: {}", e), &self.fnt);
        }

        let state = self.state.read().unwrap();
        let buf = &state.buffers[state.current_buffer];

        rx.set_color(Color::rgb(0.2, 0.2, 0.2));
        rx.fill_rect(Rect::xywh(0.0, 0.0, rx.bounds().w, 20.0));
        rx.set_color(Color::rgb(0.9, 0.65, 0.0));
        rx.draw_text(Rect::xywh(8.0, 2.0, 1000.0, 100.0),
            &format!("{} / col {}", self.mode, buf.current_column()), &self.fnt);

        self.txr.cursor_index = buf.cursor_index;
        self.txr.cursor_style = self.mode.cursor_style();
        self.txr.paint(rx, &buf.text, Rect::xywh(8.0, 20.0, rx.bounds().w, rx.bounds().h-20.0));
        
        if let Some((cmd_cur_index, pending_cmd)) = state.command_line.as_ref() {
            rx.set_color(Color::rgb(0.1, 0.1, 0.1));
            rx.fill_rect(Rect::xywh(0.0, 20.0, rx.bounds().w, 20.0));
            rx.set_color(Color::rgb(0.7, 0.35, 0.0));
            self.txr.cursor_index = *cmd_cur_index;
            self.txr.cursor_style = CursorStyle::Line;
            self.txr.paint(rx, pending_cmd, Rect::xywh(8.0, 22.0, rx.bounds().w-8.0, rx.bounds().h-20.0));
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
