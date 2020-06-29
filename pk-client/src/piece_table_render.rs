
use runic::*;
use pk_common::piece_table::PieceTable;
use crate::mode::CursorStyle;
use crate::config::{Config, Colorscheme, ColorschemeSel};

trait CursorStyleDraw {
    fn paint(&self, rx: &mut RenderContext, char_bounds: &Rect, em_bounds: &Rect);
}

impl CursorStyleDraw for CursorStyle {
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
    pub viewport_start: usize,
    pub cursor_style: CursorStyle,
    pub highlight_line: bool,
    layout_cashe: HashMap<usize, (u64, TextLayout)>
}

impl PieceTableRenderer {
    pub fn init(rx: &mut RenderContext, fnt: Font, em_bounds: Rect) -> Self {
        PieceTableRenderer {
            fnt,
            viewport_start: 0,
            em_bounds,
            cursor_style: CursorStyle::Underline,
            highlight_line: true,
            layout_cashe: HashMap::new()
        }
    }

    fn viewport_end(&self, bounds: &Rect) -> usize {
        self.viewport_start + ((bounds.h / self.em_bounds.h).floor() as usize).saturating_sub(2)
    }

    fn generate_line_layout(&self, ln: &str, global_index: usize, rx: &mut RenderContext, colors: &Colorscheme, highlights: Option<&Vec<Highlight>>) -> TextLayout {
        let layout = rx.new_text_layout(ln, &self.fnt, 10000.0, 10000.0).expect("create text layout");
        if let Some(hl) = highlights.as_ref() {
            for h in hl.iter() {
                if h.range.start > global_index + ln.len() { break; }
                if h.range.start < global_index && h.range.end < global_index { continue; }
                let range = h.range.start.saturating_sub(global_index) .. h.range.end.saturating_sub(global_index);
                //if range.len() == 0 { continue; }
                h.sort.apply_to_layout(range, rx, &layout, colors);
            }
        }
        layout
    }

    pub fn ensure_line_visible(&mut self, line: usize, bounds: Rect) {
        let viewport_end = self.viewport_end(&bounds);
        if self.viewport_start >= line { self.viewport_start = line.saturating_sub(1); }
        if viewport_end <= line { self.viewport_start += line - viewport_end; }
    }

    pub fn paint(&mut self, rx: &mut RenderContext, table: &PieceTable, cursor_index: usize,
                 config: &Config, bounds: Rect, highlights: Option<&Vec<Highlight>>)
    {
        rx.set_color(config.colors.foreground);
        let mut global_index = 0usize;
        let mut cur_pos = Point::xy(bounds.x, bounds.y); 
        let mut line_num = 0usize;
        let viewport_end = self.viewport_end(&bounds);
        let table_len = table.len();
        for p in table.pieces.iter() {
            let src = &table.sources[p.source][p.start..(p.start+p.length)];
            let mut lni = src.split('\n').peekable(); 
            loop {
                let ln = lni.next();
                if ln.is_none() { break; }
                let ln = ln.unwrap();
                if line_num < self.viewport_start {
                    if lni.peek().is_some() { line_num+=1; }
                    global_index += ln.len()+1;
                    continue;
                }
                let mut hh = DefaultHasher::new();
                ln.hash(&mut hh);
                let ln_hash = hh.finish();
                let layout = if let Some((h, ly)) 
                    = self.layout_cashe.get(&line_num)
                {
                    if *h != ln_hash {
                        // generate layout
                        let ly = self.generate_line_layout(ln, global_index, rx, &config.colors, highlights);
                        self.layout_cashe.insert(line_num, (ln_hash, ly.clone()));
                        ly
                    } else { ly.clone() }
                } else {
                    let ly = self.generate_line_layout(ln, global_index, rx, &config.colors, highlights);
                    self.layout_cashe.insert(line_num, (ln_hash, ly.clone()));
                    ly
                };
                rx.draw_text_layout(cur_pos, &layout);
                if cursor_index >= global_index && cursor_index < global_index+ln.len() ||
                    ((lni.peek().is_some() || cursor_index == table_len) && cursor_index == global_index+ln.len()) {
                    let curbounds = layout.char_bounds(cursor_index - global_index).offset(cur_pos);
                    self.cursor_style.paint(rx, &curbounds, &self.em_bounds);
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
                    cur_pos.y += text_size.h.min(self.em_bounds.h);
                    global_index += 1;
                    if line_num > viewport_end { break; }
                    //if cur_pos.y + text_size.h > bounds.h { break; }
                } else {
                    break;
                }
            }
        }
    }
}
