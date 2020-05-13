
use runic::*;
use pk_common::piece_table::PieceTable;

pub enum CursorStyle {
    Line, Block, Box, Underline
}

impl CursorStyle {
    fn paint(&self, rx: &mut RenderContext, char_bounds: &Rect, em_bounds: &Rect) {
        match self {
            CursorStyle::Line => {
                rx.fill_rect(Rect::xywh(char_bounds.x-1.0, char_bounds.y, 2.0, char_bounds.h.max(em_bounds.h)));
            },
            CursorStyle::Block => {
                rx.fill_rect(Rect::xywh(char_bounds.x, char_bounds.y, char_bounds.w.max(em_bounds.w), char_bounds.h.max(em_bounds.h)));
            },
            CursorStyle::Box => {
                rx.stroke_rect(*char_bounds, 1.0);
            },
            CursorStyle::Underline => {
                rx.fill_rect(Rect::xywh(char_bounds.x, char_bounds.y+char_bounds.h-2.0, char_bounds.w.max(em_bounds.w), 2.0));
            },
        }
    }
}

pub struct PieceTableRenderer {
    fnt: Font,
    em_bounds: Rect,
    pub cursor_index: usize,
    pub viewport_start: usize,
    pub cursor_style: CursorStyle
}

impl PieceTableRenderer {
    pub fn init(rx: &mut RenderContext, fnt: Font) -> Self {
        let ml = rx.new_text_layout("M", &fnt, 100.0, 100.0).expect("create em size layout");
        PieceTableRenderer { fnt, cursor_index: 0, viewport_start: 0, em_bounds: ml.bounds(), cursor_style: CursorStyle::Underline }
    }

    pub fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable, bounds: Rect) {
        rx.set_color(Color::white());
        let mut global_index = 0usize;
        let mut cur_pos = Point::xy(bounds.x, bounds.y); 
        let mut line_num = 0usize;
        let mut cursor_line_num = None;
        for (i,p) in table.pieces.iter().enumerate() {
            let src = &table.sources[p.source][p.start..(p.start+p.length)];
            //rx.draw_text(Rect::xywh(100.0,100.0+i as f32*30.0,1000.0,1000.0), &format!("{:?}", src.split('\n').collect::<Vec<_>>()), &self.fnt);
            let mut lni = src.split('\n').peekable(); 
            loop {
                let ln = lni.next();
                if ln.is_none() { break; }
                let ln = ln.unwrap();
                if line_num < self.viewport_start {
                    if lni.peek().is_some() { line_num+=1; }
                    global_index += ln.len()+1;
                    if self.cursor_index >= global_index && self.cursor_index <= global_index+ln.len() {
                        cursor_line_num = Some(line_num);
                    }
                    continue;
                }
                let layout = rx.new_text_layout(ln, &self.fnt, 10000.0, 10000.0).expect("create text layout");
                rx.draw_text_layout(cur_pos, &layout);
                //rx.draw_text(Rect::pnwh(cur_pos - Point::x(12.0), 100.0, 100.0), &format!("{}", global_index), &self.fnt);
                if self.cursor_index >= global_index && self.cursor_index <= global_index+ln.len() {
                    let curbounds = layout.char_bounds(self.cursor_index - global_index).offset(cur_pos);
                    cursor_line_num = Some(line_num);
                    self.cursor_style.paint(rx, &curbounds, &self.em_bounds);
                }
                let text_size = layout.bounds();
                cur_pos.x += text_size.w; 
                global_index += ln.len();
                if lni.peek().is_some() {
                    // new line
                    line_num+=1;
                    cur_pos.x = bounds.x;
                    cur_pos.y += text_size.h.min(self.em_bounds.h);
                    global_index += 1;
                    if cur_pos.y > bounds.h { break; }
                } else {
                    break;
                }
            }
        }
        // make sure the cursor is visable, if not scroll the viewport
        let cursor_line_num = cursor_line_num.unwrap_or(line_num + 1);
        if cursor_line_num < self.viewport_start { self.viewport_start -= 1; }
        if cursor_line_num > line_num { self.viewport_start += 1; }
    }
}
