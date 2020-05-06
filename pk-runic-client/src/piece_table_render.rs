
use runic::*;
use pk_common::piece_table::PieceTable;

pub struct PieceTableRenderer {
    fnt: Font,
    pub cursor_index: usize,
    pub viewport_start: usize,
}

impl PieceTableRenderer {
    pub fn init(rx: &mut RenderContext, fnt: Font) -> Self {
        PieceTableRenderer { fnt, cursor_index: 0, viewport_start: 0 }
    }

    pub fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable, position: Point) {
        rx.set_color(Color::white());
        let mut global_index = 0usize;
        let mut cur_pos = position; 
        let mut line_num = 0usize;
        for p in table.pieces.iter() {
            let src = &table.sources[p.source][p.start..(p.start+p.length)];
            //rx.draw_text(Rect::xywh(100.0,100.0+p.source as f32 *30.0,1000.0,1000.0), &format!("{:?}", src.lines().collect::<Vec<_>>()), &self.fnt);
            let mut lni = src.lines().peekable(); 
            loop {
                let ln = lni.next();
                if ln.is_none() { break; }
                let ln = ln.unwrap();
                if line_num < self.viewport_start {
                    if lni.peek().is_some() { line_num+=1; }
                    global_index += ln.len()+1;
                    continue;
                }
                let layout = rx.new_text_layout(ln, &self.fnt, 10000.0, 10000.0).expect("create text layout");
                rx.draw_text_layout(cur_pos, &layout);
                //rx.draw_text(Rect::pnwh(cur_pos - Point::x(12.0), 100.0, 100.0), &format!("{}", global_index), &self.fnt);
                if self.cursor_index >= global_index && self.cursor_index <= global_index+ln.len() {
                    //fix: the cursor doesn't appear at the end of lines because DWrite returns a
                    // zero-area rectangle, so that case needs to be checked for and a cursor
                    // rectangle manually computed
                    let curbounds = layout.char_bounds(self.cursor_index - global_index);
                    //rx.fill_rect(Rect::xywh(0.0,0.0,8.0,8.0));
                    rx.stroke_rect(curbounds.offset(cur_pos), 2.0);
                }
                let text_size = layout.bounds();
                cur_pos.x += text_size.w; 
                global_index += ln.len();
                if lni.peek().is_some() {
                    // new line
                    line_num+=1;
                    cur_pos.x = position.x;
                    cur_pos.y += text_size.h;
                    global_index += 1;
                } else {
                    break;
                }
            }
        }
    }
}
