use super::Editor;

fn items(names: &[&str]) -> Vec<(String, crate::span::Line)> {
    names.iter().map(|name| (name.to_string(), Vec::new())).collect()
}

#[test]
fn ctrl_w_takes_the_last_word_and_ctrl_u_the_line() {
    let mut editor = Editor::with("move  Archive ");
    editor.delete_word();
    assert_eq!(editor.text, "move  ");
    editor.delete_word();
    assert_eq!(editor.text, "");
    let mut editor = Editor::with("empty-trash work");
    editor.clear();
    assert_eq!(editor.text, "");
}

#[test]
fn tab_fills_the_first_choice_and_cycles_both_ways() {
    let mut editor = Editor::with("move S");
    editor.complete("Folders", items(&["Sent", "Spam", "Saved"]), true);
    assert_eq!(editor.text, "move Sent");
    assert_eq!(editor.menu.as_ref().map(|m| (m.title.as_str(), m.selected)), Some(("Folders", 0)));
    editor.complete("", Vec::new(), true);
    assert_eq!(editor.text, "move Spam");
    editor.complete("", Vec::new(), false);
    editor.complete("", Vec::new(), false);
    assert_eq!(editor.text, "move Saved", "S-Tab goes round the top");
}

#[test]
fn one_choice_is_filled_without_a_menu_and_none_changes_nothing() {
    let mut editor = Editor::with("arch");
    editor.complete("Commands", items(&["archive"]), true);
    assert_eq!((editor.text.as_str(), editor.menu.is_none()), ("archive", true));
    editor.complete("Commands", Vec::new(), true);
    assert_eq!(editor.text, "archive");
}

#[test]
fn typing_closes_the_menu() {
    let mut editor = Editor::with("a");
    editor.complete("Commands", items(&["archive", "account"]), true);
    editor.push(' ');
    assert_eq!((editor.text.as_str(), editor.menu.is_none()), ("archive ", true));
}

#[test]
fn history_is_walked_by_the_prefix_typed_and_comes_back_to_it() {
    let history: Vec<String> = ["move Archive", "trash", "move Sent"].map(String::from).to_vec();
    let mut editor = Editor::with("mo");
    editor.walk(&history, true);
    assert_eq!(editor.text, "move Sent");
    editor.walk(&history, true);
    assert_eq!(editor.text, "move Archive");
    editor.walk(&history, true);
    assert_eq!(editor.text, "move Archive", "the oldest is the end");
    editor.walk(&history, false);
    editor.walk(&history, false);
    assert_eq!(editor.text, "mo");
    let mut editor = Editor::with("zz");
    editor.walk(&history, true);
    assert_eq!(editor.text, "zz");
}
