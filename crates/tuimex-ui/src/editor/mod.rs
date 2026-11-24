//! The line behind `:` and `/`: what is typed, completion and history. The
//! caret is always at the end, as the design draws it.

use crate::span::Line;

/// The choices a completion offers, which take the rows above the status
/// line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    /// What each choice puts on the line, and how it is drawn.
    pub items: Vec<(String, Line)>,
    pub selected: usize,
    /// Where on the line the completed word starts, in bytes.
    from: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Editor {
    pub text: String,
    pub menu: Option<Menu>,
    /// While walking the history: what was typed first, and how many
    /// matching entries back the line now is.
    walk: Option<(String, usize)>,
}

impl Editor {
    pub fn with(text: &str) -> Editor {
        Editor { text: text.to_string(), ..Editor::default() }
    }

    /// An edit ends completion and the walk: the line is the user's again.
    fn edited(&mut self) {
        (self.menu, self.walk) = (None, None);
    }

    pub fn push(&mut self, c: char) {
        self.text.push(c);
        self.edited();
    }

    pub fn backspace(&mut self) {
        self.text.pop();
        self.edited();
    }

    /// `^w`: the last word, and the spaces after it.
    pub fn delete_word(&mut self) {
        let kept = self.text.trim_end_matches(' ');
        let kept = kept.rfind(' ').map_or(0, |space| space + 1);
        self.text.truncate(kept);
        self.edited();
    }

    /// `^u`.
    pub fn clear(&mut self) {
        self.text.clear();
        self.edited();
    }

    /// The word a completion would finish: what follows the last space.
    pub fn word(&self) -> &str {
        &self.text[self.text.rfind(' ').map_or(0, |space| space + 1)..]
    }

    /// Tab and S-Tab. With the menu open they move through it; otherwise
    /// `items` are the choices, the first is put on the line, and the menu
    /// opens when there is more than one.
    pub fn complete(&mut self, title: &str, items: Vec<(String, Line)>, forward: bool) {
        let menu = match self.menu.take() {
            Some(mut menu) => {
                let n = menu.items.len();
                menu.selected = (menu.selected + if forward { 1 } else { n - 1 }) % n;
                menu
            }
            None if items.is_empty() => return,
            None => Menu { title: title.to_string(), items, selected: 0, from: self.text.len() - self.word().len() },
        };
        self.text.truncate(menu.from);
        self.text.push_str(&menu.items[menu.selected].0);
        self.walk = None;
        self.menu = Some(menu).filter(|menu| menu.items.len() > 1);
    }

    /// Up (`older`) and Down through `history`, oldest first, keeping to
    /// the entries that start with what was typed before the walk began.
    pub fn walk(&mut self, history: &[String], older: bool) {
        let (typed, at) = self.walk.take().unwrap_or_else(|| (self.text.clone(), 0));
        let found: Vec<&String> = history.iter().rev().filter(|line| line.starts_with(&typed)).collect();
        let at = if older { (at + 1).min(found.len()) } else { at.saturating_sub(1) };
        self.text = if at == 0 { typed.clone() } else { found[at - 1].clone() };
        self.menu = None;
        self.walk = (at > 0).then_some((typed, at));
    }
}

#[cfg(test)]
mod tests;
