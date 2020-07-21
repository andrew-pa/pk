
use runic::*;
use pk_common::piece_table::PieceTable;
use crate::mode::CursorStyle;
use crate::config::{Config, Colorscheme, ColorschemeSel};

trait CursorStyleDraw {
    fn paint(&self, rx: &mut RenderContext, char_bounds: &Rect, em_bounds: &Rect, col: Color);
}

impl CursorStyleDraw for CursorStyle {
    fn paint(&self, rx: &mut RenderContext, char_bounds: &Rect, em_bounds: &Rect, col: Color) {
        rx.set_color(col);
        match self {
            CursorStyle::Line => {
                rx.fill_rect(Rect::xywh(char_bounds.x-1.0, char_bounds.y, 2.0, char_bounds.h.max(em_bounds.h)));
            },
            CursorStyle::Block => {
                rx.set_color(col.with_alpha(0.7));
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

#[derive(Debug)]
enum HighlightType {
    Foreground(ColorschemeSel),
}

use std::ops::Range;

#[derive(Debug)]
pub struct Highlight {
    pub range: Range<usize>,
    sort: HighlightType
}

impl Highlight {
    pub fn foreground(range: Range<usize>, sel: ColorschemeSel) -> Highlight {
        Highlight {
            range, sort: HighlightType::Foreground(sel)
        }
    }
}

impl HighlightType {
    fn apply_to_layout(&self, range: Range<usize>, rx: &mut RenderContext, txl: &TextLayout,
                               colors: &Colorscheme) {
        let cr = range.start as u32 .. range.end as u32;
        match self {
            HighlightType::Foreground(col) => txl.color_range(rx, cr, *colors.get(*col)),
        }
    }
}

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct PieceTableRenderer {
    fnt: Font,
    pub em_bounds: Rect,
    pub cursor_style: CursorStyle,
    pub highlight_line: bool,
    layout_cashe: HashMap<usize, (u64, TextLayout)>
}

impl PieceTableRenderer {
    pub fn init(rx: &mut RenderContext, fnt: Font, em_bounds: Rect) -> Self {
        PieceTableRenderer {
            fnt,
            em_bounds,
            cursor_style: CursorStyle::Underline,
            highlight_line: true,
            layout_cashe: HashMap::new()
        }
    }
    
    pub fn invalidate_layout_cashe(&mut self, rn: Range<usize>) {
        for i in rn {
            self.layout_cashe.remove(&i);
        }
    }

    pub fn viewport_end(&self, viewport_start: usize, bounds: &Rect) -> usize {
        viewport_start + ((bounds.h / self.em_bounds.h).floor() as usize).saturating_sub(2)
    }

    fn generate_line_layout(&mut self, ln: &str, global_index: usize, rx: &mut RenderContext, colors: &Colorscheme, highlights: Option<&Vec<Highlight>>) -> TextLayout {
        let mut hh = DefaultHasher::new();
        ln.hash(&mut hh);
        let ln_hash = hh.finish();
        if let Some((cashe_line_hash, ly)) = self.layout_cashe.get(&global_index) {
            if ln_hash == *cashe_line_hash {
                return ly.clone();
            }
        }
        let layout = rx.new_text_layout(ln, &self.fnt, 10000.0, 10000.0).expect("create text layout");
        if let Some(hl) = highlights.as_ref() {
            for h in hl.iter() {
                if h.range.start > global_index + ln.len() { break; }
                if h.range.start < global_index && h.range.end < global_index { continue; }
                // that subtraction of h.range.end and global_index looks real sketchy
                let range = h.range.start.saturating_sub(global_index) .. h.range.end.saturating_sub(global_index).min(ln.len());
                //if range.len() == 0 { continue; }
                h.sort.apply_to_layout(range, rx, &layout, colors);
            }
        }
        self.layout_cashe.insert(global_index, (ln_hash, layout.clone()));
        layout
    }

    pub fn ensure_line_visible(&self, viewport_start: &mut usize, line: usize, bounds: Rect) {
        let viewport_end = self.viewport_end(*viewport_start, &bounds);
        if *viewport_start >= line { *viewport_start = line.saturating_sub(1); }
        if viewport_end <= line { *viewport_start += line - viewport_end; }
    }
    
    fn paint_line_numbers(&mut self, rx: &mut RenderContext, config: &Config, cur_pos: &mut Point, line_num: usize) {
        rx.set_color(config.colors.quarter_gray);
        rx.draw_text(Rect::xywh(cur_pos.x, cur_pos.y, self.em_bounds.w*5.0, self.em_bounds.h),
            &format!("{:5}", line_num), &self.fnt);
        rx.set_color(config.colors.foreground);
        cur_pos.x += self.em_bounds.w * 7.0;
    }

    pub fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable, viewport_start: usize, cursor_index: usize,
                 config: &Config, bounds: Rect, highlights: Option<&Vec<Highlight>>, line_numbers: bool)
    {
        rx.set_color(config.colors.foreground);
        let mut global_index = 0usize;
        let mut cur_pos = Point::xy(bounds.x, bounds.y); 
        if line_numbers { cur_pos.x += self.em_bounds.w * 7.0; }
        let mut line_num = 0usize;
        let viewport_end = self.viewport_end(viewport_start, &bounds);
        let table_len = table.len();
        //self.paint_start_of_line(rx, &mut cur_pos, line_num);
        'top: for p in table.pieces.iter() {
            if p.length == 0 { continue; }
            let src = &table.sources[p.source][p.start..(p.start+p.length)];
            let mut lni = src.split('\n').peekable(); 
            loop {
                let ln = lni.next();
                if ln.is_none() { break; }
                let ln = ln.unwrap();
                if line_num < viewport_start {
                    if lni.peek().is_some() {
                        line_num+=1; 
                        global_index += 1;
                    }
                    global_index += ln.len();
                    continue;
                }
                let mut hh = DefaultHasher::new();
                ln.hash(&mut hh);
                let ln_hash = hh.finish();
                let layout = self.generate_line_layout(ln, global_index, rx, &config.colors, highlights);
                rx.draw_text_layout(cur_pos, &layout);
                if cursor_index >= global_index && cursor_index < global_index+ln.len() ||
                    ((lni.peek().is_some() || cursor_index == table_len) && cursor_index == global_index+ln.len()) {
                    let curbounds = layout.char_bounds(cursor_index - global_index).offset(cur_pos);
                    self.cursor_style.paint(rx, &curbounds, &self.em_bounds, config.colors.foreground);
                    if self.highlight_line {
                        rx.set_color(config.colors.half_gray.with_alpha(0.1));
                        rx.fill_rect(Rect::xywh(bounds.x, cur_pos.y, bounds.w, self.em_bounds.h));
                        rx.set_color(config.colors.foreground);
                    }
                }
                let text_size = layout.bounds();
                cur_pos.x += text_size.w; 
                global_index += ln.len();
                if lni.peek().is_some() {
                    // new line
                    line_num+=1;
                    cur_pos.x = bounds.x;
                    if line_numbers { self.paint_line_numbers(rx, config, &mut cur_pos, line_num); }
                    cur_pos.y += text_size.h.min(self.em_bounds.h);
                    global_index += 1;
                    if line_num > viewport_end { break 'top; }
                    //if cur_pos.y + text_size.h > bounds.h { break; }
                } else {
                    break;
                }
            }
        }
    }
}
