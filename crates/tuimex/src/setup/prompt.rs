//! The questions `tuimex setup` asks: lines on a terminal, with no screen
//! to draw, so that they work over SSH and under a script (PLAN §6.7).

use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use tuimex_engine::Secret;

pub trait Prompt {
    fn say(&mut self, text: &str);
    /// The answer to `question`, without the space around it.
    fn ask(&mut self, question: &str) -> Result<String>;
    /// As `ask`, for an answer that must not be shown as it is typed.
    fn secret(&mut self, question: &str) -> Result<Secret>;
}

/// Standard input and output.
pub struct Terminal;

fn line(question: &str) -> Result<String> {
    print!("{question} ");
    io::stdout().flush().context("writing to the terminal")?;
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer).context("reading an answer")? == 0 {
        bail!("the answers ended before the questions did; nothing was saved");
    }
    Ok(answer.trim().to_string())
}

/// Keys read one at a time with the terminal in raw mode, which is what
/// stops it echoing them: `crossterm` is how tuimex already talks to the
/// terminal, and it has no other way to turn echo off.
fn keys() -> Result<String> {
    // Room for any password, so the text is never copied by growing.
    let mut typed = String::with_capacity(1024);
    loop {
        let Event::Key(key) = event::read().context("reading the keyboard")? else { continue };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        match key.code {
            KeyCode::Enter => return Ok(typed),
            KeyCode::Char('c' | 'd') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                bail!("interrupted; nothing was saved")
            }
            KeyCode::Char(c) => typed.push(c),
            KeyCode::Backspace => _ = typed.pop(),
            _ => {}
        }
    }
}

impl Prompt for Terminal {
    fn say(&mut self, text: &str) {
        println!("{text}");
    }

    fn ask(&mut self, question: &str) -> Result<String> {
        line(question)
    }

    fn secret(&mut self, question: &str) -> Result<Secret> {
        // With no terminal there is no echo to turn off: the answer is a
        // line from whatever is piped in.
        if !io::stdin().is_terminal() {
            return line(question).map(Secret::new);
        }
        print!("{question} ");
        io::stdout().flush().context("writing to the terminal")?;
        terminal::enable_raw_mode().context("turning the terminal's echo off")?;
        let typed = keys();
        // Whatever was typed, the terminal is given back as it was found.
        let restored = terminal::disable_raw_mode().context("turning the terminal's echo back on");
        println!();
        restored?;
        typed.map(Secret::new)
    }
}

/// A yes or no, with what Enter alone means shown in capitals.
pub fn yes(prompt: &mut dyn Prompt, question: &str, default: bool) -> Result<bool> {
    loop {
        let answer = prompt.ask(&format!("{question} [{}]", if default { "Y/n" } else { "y/N" }))?;
        match answer.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => prompt.say("Answer y or n."),
        }
    }
}

/// An answer, or `default` for Enter alone.
pub fn or(prompt: &mut dyn Prompt, question: &str, default: &str) -> Result<String> {
    let answer = prompt.ask(&format!("{question} [{default}]:"))?;
    Ok(if answer.is_empty() { default.to_string() } else { answer })
}

/// A number from 1 to `most`; Enter alone is `default`, which may be none.
pub fn number(prompt: &mut dyn Prompt, question: &str, most: usize, default: Option<usize>) -> Result<Option<usize>> {
    loop {
        let shown = default.map_or("none".to_string(), |n| n.to_string());
        let answer = prompt.ask(&format!("{question} [{shown}]:"))?;
        match answer.parse::<usize>() {
            _ if answer.is_empty() => return Ok(default),
            Ok(n) if (1..=most).contains(&n) => return Ok(Some(n)),
            _ => prompt.say(&format!("Answer with a number from 1 to {most}.")),
        }
    }
}
