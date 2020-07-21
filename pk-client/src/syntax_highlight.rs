use regex::{Regex, Match};
use crate::piece_table_render::Highlight;
use crate::config::ColorschemeSel;
use std::collections::HashMap;
use std::ops::Range;

struct KeywordMatchIter<'t> {
    text: &'t str,
    keyword: &'t str,
    current_index: usize
}

impl<'t> Iterator for KeywordMatchIter<'t> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index >= self.text.len() { return None; }
        self.text[self.current_index..].find(self.keyword).map(|ix| {
            let ix = ix + self.current_index;
            self.current_index = ix + self.keyword.len();
            ix .. self.current_index
        })
    }
}

pub enum HighlightRule {
    Keyword(String),
    RegexMatch(Regex)
}

impl HighlightRule {
    fn matches<'t>(&'t self, text: &'t str) -> Box<dyn Iterator<Item=Range<usize>> + 't> {
        match self {
            Self::Keyword(kw) => {
                Box::new(KeywordMatchIter {
                    text, keyword: kw.as_str(), current_index: 0
                })
            },
            Self::RegexMatch(r) => {
                Box::new(r.find_iter(text).map(|m| m.range()))
            }
        }
    }
}

#[derive(Hash, Eq, PartialEq)]
pub enum LexicalItemType {
    Comment, Keyword,
    Strings, Character, Number, Identifier,
    Operator, Type,
    Conditional, Repeat,
    Macro, Label, Special
}

pub struct SyntaxRules {
    pub highlight_rules: Vec<(LexicalItemType, HighlightRule)>
}

fn overlaps<T: Ord>(a: &Range<T>, b: &Range<T>) -> bool {
    (b.start >= a.start && b.start < a.end) ||
    (b.end >= a.start && b.end < a.end) 
}

impl SyntaxRules {
    pub fn apply(&self, text: &str, color_map: &HashMap<LexicalItemType, ColorschemeSel>) -> Vec<Highlight> {
        let mut hi: Vec<Highlight> = Vec::new();
        for rule in self.highlight_rules.iter() {
            for m in rule.1.matches(text) {
                if hi.iter().any(|h| overlaps(&h.range, &m)) { continue; }
                hi.push(Highlight::foreground(m, *color_map.get(&rule.0).unwrap()));
            }
        }
        hi.sort_unstable_by(|a, b| a.range.start.cmp(&b.range.start));
        hi
    }
}

use syntect::parsing::*;
use syntect::highlighting::{ScopeSelector,ScopeSelectors};
use crate::buffer;

mod syntect_highlighter {
//! Iterators and data structures for transforming parsing information into styled text.
//! but modified to generate styles that use ColorschemeSels so that colors are uniform throughout pk

// Code based on https://github.com/defuz/sublimate/blob/master/src/core/syntax/highlighter.rs
// released under the MIT license by @defuz

use std::iter::Iterator;
use std::ops::Range;

use syntect::parsing::{Scope, ScopeStack, BasicScopeStackOp, ScopeStackOp, MatchPower, ATOM_LEN_BITS};
use syntect::highlighting::{ScopeSelector, ScopeSelectors};
use super::ColorschemeSel;
//use syntect::highlighting::{Theme, ThemeItem};
//use syntect::highlighting::{Color, FontStyle, Style, StyleModifier};

// structs that uses pk's highlighting system instead of syntect's
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StyleModifier {
    foreground: Option<ColorschemeSel>,
    background: Option<ColorschemeSel>,
    font_style: Option<FontStyle>
}

impl StyleModifier {
    pub fn fg(col: ColorschemeSel) -> StyleModifier {
        StyleModifier {
            foreground: Some(col),
            background: None,
            font_style: None
        }
    }

    fn apply(self, other: StyleModifier) -> StyleModifier {
        StyleModifier {
            foreground: other.foreground.or(self.foreground),
            background: other.background.or(self.background),
            font_style: other.font_style.or(self.font_style)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeItem {
    pub scope: ScopeSelectors,
    pub style: StyleModifier 
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub scopes: Vec<ThemeItem>
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FontStyle {
    Normal
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Style {
    pub foreground: ColorschemeSel,
    pub background: ColorschemeSel,
    pub font_style: FontStyle
}

impl Default for Style {
    fn default() -> Self {
        Style {
            foreground: ColorschemeSel::Foreground,
            background: ColorschemeSel::Background,
            font_style: FontStyle::Normal
        }
    }
}

/// Basically a wrapper around a `Theme` preparing it to be used for highlighting.
/// This is part of the API to preserve the possibility of caching
/// matches of the selectors of the theme on various scope paths
/// or setting up some kind of accelerator structure.
///
/// So for now this does very little but eventually if you keep it around between
/// highlighting runs it will preserve its cache.
#[derive(Debug)]
pub struct Highlighter<'a> {
    theme: &'a Theme,
    /// Cache of the selectors in the theme that are only one scope
    /// In most themes this is the majority, hence the usefullness
    single_selectors: Vec<(Scope, StyleModifier)>,
    multi_selectors: Vec<(ScopeSelector, StyleModifier)>,
    // TODO single_cache: HashMap<Scope, StyleModifier, BuildHasherDefault<FnvHasher>>,
}

/// Keeps a stack of scopes and styles as state between highlighting different lines.
/// If you are highlighting an entire file you create one of these at the start and use it
/// all the way to the end.
///
/// # Caching
///
/// One reason this is exposed is that since it implements `Clone` you can actually cache
/// these (probably along with a `ParseState`) and only re-start highlighting from the point of a change.
/// You could also do something fancy like only highlight a bit past the end of a user's screen and resume
/// highlighting when they scroll down on large files.
///
/// Alternatively you can save space by caching only the `path` field of this struct
/// then re-create the `HighlightState` when needed by passing that stack as the `initial_stack`
/// parameter to the `new` method. This takes less space but a small amount of time to re-create the style stack.
///
/// **Note:** Caching is for advanced users who have tons of time to maximize performance or want to do so eventually.
/// It is not recommended that you try caching the first time you implement highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightState {
    styles: Vec<Style>,
    single_caches: Vec<ScoredStyle>,
    pub path: ScopeStack,
}

/// Highlights a line of parsed code given a `HighlightState`
/// and line of changes from the parser.
/// Yields the `Style`, the `text` as well as the `Range` of the text in the source string.
///
/// It splits a line of text into different pieces each with a `Style`
#[derive(Debug)]
pub struct RangedHighlightIterator<'a, 'b> {
    index: usize,
    pos: usize,
    changes: &'a [(usize, ScopeStackOp)],
    text: &'b str,
    highlighter: &'a Highlighter<'a>,
    state: &'a mut HighlightState,
}

/// Highlights a line of parsed code given a `HighlightState`
/// and line of changes from the parser.
/// This is a backwards compatible shim on top of the `RangedHighlightIterator` which only
/// yields the `Style` and the `Text` of the token, not the range.
///
/// It splits a line of text into different pieces each with a `Style`
#[derive(Debug)]
pub struct HighlightIterator<'a, 'b> {
    ranged_iterator: RangedHighlightIterator<'a, 'b>
}

impl HighlightState {
    /// Note that the `Highlighter` is not stored, it is used to construct the initial
    /// stack of styles. Most of the time you'll want to pass an empty stack as `initial_stack`
    /// but see the docs for `HighlightState` for discussion of advanced caching use cases.
    pub fn new(highlighter: &Highlighter<'_>, initial_stack: ScopeStack) -> HighlightState {
        let mut styles = vec![highlighter.get_default()];
        let mut single_caches = vec![ScoredStyle::from_style(styles[0])];
        for i in 0..initial_stack.len() {
            let prefix = initial_stack.bottom_n(i + 1);
            let new_cache = highlighter.update_single_cache_for_push(&single_caches[i], prefix);
            styles.push(highlighter.finalize_style_with_multis(&new_cache, prefix));
            single_caches.push(new_cache);
        }

        HighlightState {
            styles,
            single_caches,
            path: initial_stack,
        }
    }
}

impl<'a, 'b> RangedHighlightIterator<'a, 'b> {
    pub fn new(state: &'a mut HighlightState,
               changes: &'a [(usize, ScopeStackOp)],
               text: &'b str,
               highlighter: &'a Highlighter<'_>)
               -> RangedHighlightIterator<'a, 'b> {
        RangedHighlightIterator {
            index: 0,
            pos: 0,
            changes,
            text,
            highlighter,
            state,
        }
    }
}

impl<'a, 'b> Iterator for RangedHighlightIterator<'a, 'b> {
    type Item = (Style, &'b str, Range<usize>);

    /// Yields the next token of text and the associated `Style` to render that text with.
    /// the concatenation of the strings in each token will make the original string.
    fn next(&mut self) -> Option<(Style, &'b str, Range<usize>)> {
        if self.pos == self.text.len() && self.index >= self.changes.len() {
            return None;
        }
        let (end, command) = if self.index < self.changes.len() {
            self.changes[self.index].clone()
        } else {
            (self.text.len(), ScopeStackOp::Noop)
        };
        // println!("{} - {:?}   {}:{}", self.index, self.pos, self.state.path.len(), self.state.styles.len());
        let style = *self.state.styles.last().unwrap_or(&Style::default());
        let text = &self.text[self.pos..end];
        let range = Range { start: self.pos, end: end };
        {
            // closures mess with the borrow checker's ability to see different struct fields
            let m_path = &mut self.state.path;
            let m_styles = &mut self.state.styles;
            let m_caches = &mut self.state.single_caches;
            let highlighter = &self.highlighter;
            m_path.apply_with_hook(&command, |op, cur_stack| {
                // println!("{:?} - {:?}", op, cur_stack);
                match op {
                    BasicScopeStackOp::Push(_) => {
                        // we can push multiple times so this might have changed
                        let new_cache = {
                            if let Some(prev_cache) = m_caches.last() {
                                highlighter.update_single_cache_for_push(prev_cache, cur_stack)
                            } else {
                                highlighter.update_single_cache_for_push(&ScoredStyle::from_style(highlighter.get_default()), cur_stack)
                            }
                        };
                        m_styles.push(highlighter.finalize_style_with_multis(&new_cache, cur_stack));
                        m_caches.push(new_cache);
                    }
                    BasicScopeStackOp::Pop => {
                        m_styles.pop();
                        m_caches.pop();
                    }
                }
            });
        }
        self.pos = end;
        self.index += 1;
        if text.is_empty() {
            self.next()
        } else {
            Some((style, text, range))
        }
    }
}
impl<'a, 'b> HighlightIterator<'a, 'b> {
    pub fn new(state: &'a mut HighlightState,
               changes: &'a [(usize, ScopeStackOp)],
               text: &'b str,
               highlighter: &'a Highlighter<'_>)
        -> HighlightIterator<'a, 'b> {
            HighlightIterator {
                ranged_iterator: RangedHighlightIterator {
                    index: 0,
                    pos: 0,
                    changes,
                    text,
                    highlighter,
                    state
                }
            }
    }
}

impl<'a, 'b> Iterator for HighlightIterator<'a, 'b> {
    type Item = (Style, &'b str);

    /// Yields the next token of text and the associated `Style` to render that text with.
    /// the concatenation of the strings in each token will make the original string.
    fn next(&mut self) -> Option<(Style, &'b str)> {
        self.ranged_iterator.next().map(|e| (e.0, e.1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredStyle {
    pub foreground: (MatchPower, ColorschemeSel),
    pub background: (MatchPower, ColorschemeSel),
    pub font_style: (MatchPower, FontStyle),
}

#[inline]
fn update_scored<T: Clone>(scored: &mut (MatchPower, T), update: &Option<T>, score: MatchPower) {
    if score > scored.0 {
        if let Some(u) = update {
            scored.0 = score;
            scored.1 = u.clone();
        }
    }
}

impl ScoredStyle {
    fn apply(&mut self, other: &StyleModifier, score: MatchPower) {
        update_scored(&mut self.foreground, &other.foreground, score);
        update_scored(&mut self.background, &other.background, score);
        update_scored(&mut self.font_style, &other.font_style, score);
    }

    fn to_style(&self) -> Style {
        Style {
            foreground: self.foreground.1,
            background: self.background.1,
            font_style: self.font_style.1,
        }
    }

    fn from_style(style: Style) -> ScoredStyle {
        ScoredStyle {
            foreground: (MatchPower(-1.0), style.foreground),
            background: (MatchPower(-1.0), style.background),
            font_style: (MatchPower(-1.0), style.font_style),
        }
    }
}

impl<'a> Highlighter<'a> {
    pub fn new(theme: &'a Theme) -> Highlighter<'a> {
        let mut single_selectors = Vec::new();
        let mut multi_selectors = Vec::new();
        for item in &theme.scopes {
            for sel in &item.scope.selectors {
                if let Some(scope) = sel.extract_single_scope() {
                    single_selectors.push((scope, item.style));
                } else {
                    multi_selectors.push((sel.clone(), item.style));
                }
            }
        }
        // So that deeper matching selectors get checked first
        single_selectors.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        Highlighter {
            theme,
            single_selectors,
            multi_selectors,
        }
    }

    /// The default style in the absence of any matched rules.
    /// Basically what plain text gets highlighted as.
    pub fn get_default(&self) -> Style {
        Style::default()
    }

    fn update_single_cache_for_push(&self, cur: &ScoredStyle, path: &[Scope]) -> ScoredStyle {
        let mut new_style = cur.clone();

        let last_scope = path[path.len() - 1];
        for &(scope, ref modif) in self.single_selectors.iter().filter(|a| a.0.is_prefix_of(last_scope)) {
            let single_score = f64::from(scope.len()) *
                               f64::from(ATOM_LEN_BITS * ((path.len() - 1) as u16)).exp2();
            new_style.apply(modif, MatchPower(single_score));
        }

        new_style
    }

    fn finalize_style_with_multis(&self, cur: &ScoredStyle, path: &[Scope]) -> Style {
        let mut new_style = cur.clone();

        let mult_iter = self.multi_selectors
            .iter()
            .filter_map(|&(ref sel, ref style)| sel.does_match(path).map(|score| (score, style)));
        for (score, ref modif) in mult_iter {
            new_style.apply(modif, score);
        }

        new_style.to_style()
    }

    /// Returns the fully resolved style for the given stack.
    ///
    /// This operation is convenient but expensive. For reasonable performance,
    /// the caller should be caching results.
    pub fn style_for_stack(&self, stack: &[Scope]) -> Style {
        let mut single_cache = ScoredStyle::from_style(self.get_default());
        for i in 0..stack.len() {
            single_cache = self.update_single_cache_for_push(&single_cache, &stack[0..i+1]);
        }
        self.finalize_style_with_multis(&single_cache, stack)
    }

    /// Returns a `StyleModifier` which, if applied to the default style,
    /// would generate the fully resolved style for this stack.
    ///
    /// This is made available to applications that are using syntect styles
    /// in combination with style information from other sources.
    ///
    /// This operation is convenient but expensive. For reasonable performance,
    /// the caller should be caching results. It's likely slower than style_for_stack.
    pub fn style_mod_for_stack(&self, path: &[Scope]) -> StyleModifier {
        let mut matching_items : Vec<(MatchPower, &ThemeItem)> = self.theme
            .scopes
            .iter()
            .filter_map(|item| {
                item.scope
                    .does_match(path)
                    .map(|score| (score, item))
            })
            .collect();
        matching_items.sort_by_key(|&(score, _)| score);
        let sorted = matching_items.iter()
            .map(|(_, item)| item);

        let mut modifier = StyleModifier {
            background: None,
            foreground: None,
            font_style: None,
        };
        for item in sorted {
            modifier = modifier.apply(item.style);
        }
        modifier
    }
}
}

pub struct Highlighter {
    synset: SyntaxSet,
    color_sel: syntect_highlighter::Theme
}

impl Highlighter {
    pub fn from_toml(val: Option<&toml::Value>) -> Highlighter {
        use std::str::FromStr;
        Highlighter {
            synset: SyntaxSet::load_defaults_nonewlines(),
            // error handling here could be a lot better
            color_sel: syntect_highlighter::Theme {
                scopes: val.and_then(|val| val.as_array()
                            .map(|rules| rules.iter()
                                .map(|rule| syntect_highlighter::ThemeItem {
                                    scope: ScopeSelectors::from_str(rule.get("scope").and_then(toml::Value::as_str).unwrap()).unwrap(),
                                    style: syntect_highlighter::StyleModifier::fg(
                                        ColorschemeSel::from_toml(rule.get("style").unwrap()).unwrap())
                                }).collect())).unwrap_or_else(Vec::new)
            }
        }
    }

    pub fn compute_highlighting(&self, buf: &buffer::Buffer) -> Vec<Highlight> {
        // dbg!(&self.color_sel);
        let mut parser = ParseState::new(buf.path.extension().and_then(|s| s.to_str())
            .and_then(|ext| self.synset.find_syntax_by_extension(ext))
            .unwrap_or_else(|| self.synset.find_syntax_plain_text()));
        let mut hi = Vec::new();
        let hl = syntect_highlighter::Highlighter::new(&self.color_sel);
        let mut hlstate = syntect_highlighter::HighlightState::new(&hl, ScopeStack::new());
        let mut gi = 0;
        //let tx = buf.text.text();
        //println!("highlighting text:\n{}", tx);
        for (ops, ln) in buf.text.text().lines()
            .map(|ln| (parser.parse_line(ln, &self.synset), ln))
        {
            hi.extend(syntect_highlighter::RangedHighlightIterator::new(&mut hlstate, &ops[..], ln, &hl)
                .map(|(style, _, range)| {
                    Highlight::foreground((range.start + gi) .. (range.end + gi), style.foreground)
                }));
            gi += ln.len() + 1;
        }
        hi
    }
}

