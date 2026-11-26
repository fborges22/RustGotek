use crate::operations::{OPS, Operation, Page};
use console::{Key, Term, truncate_str};
use std::io::{self, IsTerminal};

const PAGES: &[(&str, Page)] = &[
    ("Floppy images", Page::Images),
    ("Gotek USB banks", Page::Banks),
    ("Mounted media", Page::Media),
    ("Indexed slots", Page::Slots),
    ("Folder backups", Page::Backups),
    ("FlashFloppy settings", Page::Config),
    ("Firmware", Page::Firmware),
];

/// Redraw only the menu so command output remains available in scrollback.
fn select(
    term: &Term,
    title: &str,
    items: &[&str],
    selected: &mut usize,
) -> io::Result<Option<usize>> {
    term.write_line(title)?;
    term.write_line("Up/Down to choose, Enter to select, Esc or q to go back")?;
    loop {
        let (height, width) = term.size();
        let visible = items
            .len()
            .min(usize::from(height.saturating_sub(4).max(1)));
        let start = (*selected + 1).saturating_sub(visible);
        for (i, label) in items[start..start + visible].iter().enumerate() {
            let marker = if start + i == *selected { ">" } else { " " };
            let line = format!("{marker} {label}");
            term.write_line(&truncate_str(
                &line,
                usize::from(width.saturating_sub(1)),
                "",
            ))?;
        }
        let key = term.read_key()?;
        term.clear_last_lines(visible)?;
        match key {
            Key::ArrowUp | Key::Char('k') => {
                *selected = (*selected + items.len() - 1) % items.len()
            }
            Key::ArrowDown | Key::Char('j') => *selected = (*selected + 1) % items.len(),
            Key::Enter => return Ok(Some(*selected)),
            Key::Escape | Key::Char('q') => return Ok(None),
            _ => {}
        }
    }
}

fn field(term: &Term, label: &str, default: &str) -> io::Result<Option<String>> {
    loop {
        term.write_str(label)?;
        if !default.is_empty() {
            term.write_str(&format!(" [{default}]"))?;
        }
        term.write_str(": ")?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let value = line.trim_end_matches(['\r', '\n']);
        if value == ":back" {
            return Ok(None);
        }
        let value = if value.is_empty() { default } else { value };
        if !value.trim().is_empty() {
            return Ok(Some(value.to_owned()));
        }
        term.write_line("Enter a value or type :back to cancel.")?;
    }
}

fn operation(term: &Term, op: &Operation, media: &mut String) -> crate::Result<()> {
    term.write_line(op.title)?;
    term.write_line(op.description)?;
    let mut args = vec![op.command.to_owned()];
    if op.media || !op.fields.is_empty() {
        term.write_line("Enter accepts the displayed default. Type :back to cancel.")?;
    }
    if op.media {
        let Some(path) = field(term, "Mounted media folder", media)? else {
            return Ok(());
        };
        media.clone_from(&path);
        args.push(path);
    }
    for (index, f) in op.fields.iter().enumerate() {
        if op.command == "usb-init" && index == 0 {
            let disks = crate::usb::detect();
            if !disks.is_empty() {
                let mut choices: Vec<_> = disks.iter().map(|(_, label)| label.as_str()).collect();
                choices.push("Enter device path manually");
                let Some(choice) = select(term, "Choose the USB drive to erase", &choices, &mut 0)?
                else {
                    return Ok(());
                };
                if let Some((path, _)) = disks.get(choice) {
                    args.push(path.clone());
                    continue;
                }
            }
        }
        let Some(value) = field(term, f.label, f.default)? else {
            return Ok(());
        };
        args.push(value);
    }
    if op.confirm {
        if op.media {
            term.write_line(&format!("Mounted media folder: {media}"))?;
        }
        for (f, value) in op.fields.iter().zip(&args[1 + usize::from(op.media)..]) {
            term.write_line(&format!("{}: {value}", f.label))?;
        }
        if select(
            term,
            "Review the target and paths",
            &["Cancel", op.title],
            &mut 0,
        )? != Some(1)
        {
            return Ok(());
        }
    }
    if crate::operations::requires_write_flag(op.command) {
        args.push("--confirm-write".into());
    }
    match crate::run(&args) {
        Ok(()) => term.write_line("Done.")?,
        Err(error) => term.write_line(&format!("Error: {error}"))?,
    }
    Ok(())
}

pub fn launch() -> crate::Result<()> {
    let term = Term::stdout();
    if !io::stdin().is_terminal() || !term.is_term() {
        return Err("the terminal menu needs interactive input and output; use rustgotek --help for CLI commands".into());
    }
    let titles: Vec<_> = PAGES.iter().map(|(title, _)| *title).collect();
    let mut category = 0;
    let mut media = String::from(".");
    while let Some(i) = select(&term, "RustGotek", &titles, &mut category)? {
        let (title, page) = PAGES[i];
        let ops: Vec<_> = OPS.iter().filter(|op| op.page == page).collect();
        let names: Vec<_> = ops.iter().map(|op| op.title).collect();
        let mut choice = 0;
        while let Some(j) = select(&term, title, &names, &mut choice)? {
            operation(&term, ops[j], &mut media)?;
        }
    }
    Ok(())
}
