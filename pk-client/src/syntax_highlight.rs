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
