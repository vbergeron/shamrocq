use std::collections::HashMap;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::disasm::{Disassembly, Item};
use crate::style as S;

const SIDEBAR_WIDTH: u16 = 50;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Sidebar,
    Code,
}

struct App {
    // Code panel
    items: Vec<Line<'static>>,
    jump_targets: Vec<Option<usize>>,
    code_state: ListState,
    history: Vec<usize>,
    title: String,

    // Sidebar
    focus: Focus,
    globals: Vec<(String, u16)>,
    global_display_targets: Vec<Option<usize>>,
    global_state: ListState,
    tags: Vec<String>,
}

impl App {
    fn new(d: Disassembly) -> Self {
        let mut addr_to_item: HashMap<u16, usize> = HashMap::new();
        for (i, item) in d.items.iter().enumerate() {
            let addr = match item {
                Item::FnLabel { addr, .. } | Item::BranchLabel { addr, .. } => Some(*addr),
                Item::Instr { addr, .. } => Some(*addr as u16),
                Item::MatchEntry { .. } => None,
            };
            if let Some(a) = addr {
                addr_to_item.entry(a).or_insert(i);
            }
        }

        let mut items: Vec<Line<'static>> = Vec::new();
        let mut jump_targets: Vec<Option<usize>> = Vec::new();
        let blank = Line::from("");

        let mut item_jumps: Vec<Option<usize>> = Vec::with_capacity(d.items.len());
        for item in &d.items {
            let target = match item {
                Item::MatchEntry { target, .. } => addr_to_item.get(target).copied(),
                Item::Instr { operands, .. } => {
                    parse_code_addr(operands).and_then(|a| addr_to_item.get(&a).copied())
                }
                _ => None,
            };
            item_jumps.push(target);
        }

        let mut item_to_display: Vec<usize> = Vec::with_capacity(d.items.len());
        for (i, item) in d.items.iter().enumerate() {
            match item {
                Item::FnLabel { .. } if i > 0 => {
                    items.push(blank.clone());
                    jump_targets.push(None);
                    items.push(blank.clone());
                    jump_targets.push(None);
                }
                Item::BranchLabel { .. } => {
                    items.push(blank.clone());
                    jump_targets.push(None);
                }
                _ => {}
            }
            item_to_display.push(items.len());
            items.push(render_item(item, &d.tags, item_jumps[i].is_some()));
            jump_targets.push(None);
        }

        for (item_idx, display_idx) in item_to_display.iter().enumerate() {
            if let Some(target_item) = item_jumps[item_idx] {
                jump_targets[*display_idx] = Some(item_to_display[target_item]);
            }
        }

        // Build globals sidebar data with jump targets into the code display
        let globals: Vec<(String, u16)> = d.globals.iter()
            .map(|g| (g.name.clone(), g.offset))
            .collect();
        let global_display_targets: Vec<Option<usize>> = d.globals.iter()
            .map(|g| {
                addr_to_item.get(&g.offset)
                    .and_then(|&item_idx| item_to_display.get(item_idx).copied())
            })
            .collect();

        let title = format!(
            " {} | Bytecode version {} | {} bytes | {} code ",
            d.filename, d.version, d.blob_len, d.code_len,
        );

        let mut code_state = ListState::default();
        code_state.select(Some(0));
        let mut global_state = ListState::default();
        if !globals.is_empty() {
            global_state.select(Some(0));
        }
        App {
            items,
            jump_targets,
            code_state,
            history: Vec::new(),
            title,
            focus: Focus::Code,
            globals,
            global_display_targets,
            global_state,
            tags: d.tags,
        }
    }

    // --- Code panel navigation ---

    fn code_selected(&self) -> usize {
        self.code_state.selected().unwrap_or(0)
    }

    fn code_select(&mut self, idx: usize) {
        self.code_state.select(Some(idx.min(self.items.len().saturating_sub(1))));
    }

    fn code_next(&mut self) {
        let i = self.code_selected();
        if i + 1 < self.items.len() {
            self.code_select(i + 1);
        }
    }

    fn code_prev(&mut self) {
        let i = self.code_selected();
        if i > 0 {
            self.code_select(i - 1);
        }
    }

    fn code_page_down(&mut self, height: usize) {
        let i = (self.code_selected() + height).min(self.items.len().saturating_sub(1));
        self.code_select(i);
    }

    fn code_page_up(&mut self, height: usize) {
        let i = self.code_selected().saturating_sub(height);
        self.code_select(i);
    }

    fn code_jump_forward(&mut self) {
        let sel = self.code_selected();
        if let Some(target) = self.jump_targets[sel] {
            self.history.push(sel);
            self.code_select(target);
            *self.code_state.offset_mut() = target.saturating_sub(10);
        }
    }

    fn code_jump_back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.code_select(prev);
            *self.code_state.offset_mut() = prev.saturating_sub(20);
        }
    }

    // --- Sidebar navigation ---

    fn sidebar_selected(&self) -> usize {
        self.global_state.selected().unwrap_or(0)
    }

    fn sidebar_len(&self) -> usize {
        self.globals.len() + 1 + self.tags.len() // +1 for the separator
    }

    fn sidebar_next(&mut self) {
        let sel = self.sidebar_selected();
        if sel + 1 < self.sidebar_len() {
            self.sidebar_select(sel + 1);
        }
    }

    fn sidebar_prev(&mut self) {
        let sel = self.sidebar_selected();
        if sel > 0 {
            self.sidebar_select(sel - 1);
        }
    }

    fn sidebar_select(&mut self, idx: usize) {
        let idx = idx.min(self.sidebar_len().saturating_sub(1));
        self.global_state.select(Some(idx));
    }

    fn sidebar_jump_to_code(&mut self) {
        let sel = self.sidebar_selected();
        if sel < self.globals.len() {
            if let Some(target) = self.global_display_targets[sel] {
                self.focus = Focus::Code;
                self.history.push(self.code_selected());
                self.code_select(target);
                *self.code_state.offset_mut() = target.saturating_sub(10);
            }
        }
    }

    fn sidebar_current_is_jumpable(&self) -> bool {
        let sel = self.sidebar_selected();
        sel < self.globals.len() && self.global_display_targets[sel].is_some()
    }
}

fn parse_code_addr(operands: &str) -> Option<u16> {
    let idx = operands.find("code+0x")?;
    let s = &operands[idx + 7..];
    let hex: String = s.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    u16::from_str_radix(&hex, 16).ok()
}

fn render_item(item: &Item, tags: &[String], jumpable: bool) -> Line<'static> {
    match item {
        Item::FnLabel { addr, name, comment } => {
            let mut spans = vec![
                Span::styled(format!("  {:04X}  ", addr), S::ADDR),
                Span::styled(format!("<{}>:", name), S::FN_LABEL),
            ];
            if !comment.is_empty() {
                spans.push(Span::styled(format!(" ; {}", comment), S::COMMENT));
            }
            Line::from(spans)
        }
        Item::BranchLabel { addr, name } => {
            Line::from(vec![
                Span::styled(format!("  {:04X}  ", addr), S::ADDR),
                Span::styled(format!("{}:", name), S::BRANCH_LABEL),
            ])
        }
        Item::Instr { addr, mnemonic, operands, annotation } => {
            let is_branch = matches!(*mnemonic, "MATCH" | "MATCH2" | "JMP");
            let mn_style = if is_branch {
                S::MNEMONIC_BRANCH
            } else if jumpable {
                S::MNEMONIC_JUMP
            } else {
                S::MNEMONIC
            };
            let mut spans = vec![
                Span::styled(format!("  {:04X}  ", addr), S::ADDR),
                Span::styled(format!("{:<13}", mnemonic), mn_style),
            ];
            if !operands.is_empty() {
                spans.push(Span::styled(
                    format!("{:<17}", colorize_operands_spans(operands, tags)),
                    S::OPERAND,
                ));
            }
            if !annotation.is_empty() {
                spans.push(Span::styled(format!("; {}", annotation), S::COMMENT));
            }
            Line::from(spans)
        }
        Item::MatchEntry { tag, tag_name, arity, target } => {
            let target_style = if jumpable { S::MATCH_TARGET_JUMP } else { S::MATCH_TARGET };
            let tag_str = match tag_name {
                Some(name) if !name.is_empty() => format!("{} ({})", tag, name),
                _ => format!("{}", tag),
            };
            Line::from(vec![
                Span::raw("        "),
                Span::styled("| ", S::MATCH_PIPE),
                Span::styled(format!("tag={:<20}", tag_str), S::OPERAND),
                Span::styled(format!("arity={:<4}", arity), S::MATCH_ARITY),
                Span::styled(format!("0x{:04X}", target), target_style),
            ])
        }
    }
}

fn colorize_operands_spans(ops: &str, _tags: &[String]) -> String {
    ops.to_string()
}

pub fn run(d: Disassembly) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(d);

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc => break,
                KeyCode::Tab => {
                    app.focus = match app.focus {
                        Focus::Code => Focus::Sidebar,
                        Focus::Sidebar => Focus::Code,
                    };
                }
                KeyCode::Down => match app.focus {
                    Focus::Code => app.code_next(),
                    Focus::Sidebar => app.sidebar_next(),
                },
                KeyCode::Up => match app.focus {
                    Focus::Code => app.code_prev(),
                    Focus::Sidebar => app.sidebar_prev(),
                },
                KeyCode::PageDown => if app.focus == Focus::Code { app.code_page_down(20) },
                KeyCode::PageUp => if app.focus == Focus::Code { app.code_page_up(20) },
                KeyCode::Right | KeyCode::Enter => match app.focus {
                    Focus::Code => app.code_jump_forward(),
                    Focus::Sidebar => app.sidebar_jump_to_code(),
                },
                KeyCode::Left => if app.focus == Focus::Code { app.code_jump_back() },
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ]).split(f.area());

    // Title bar
    let header = Paragraph::new(Line::from(app.title.clone())).style(S::BAR);
    f.render_widget(header, outer[0]);

    // Main area: sidebar | code
    let columns = Layout::horizontal([
        Constraint::Length(SIDEBAR_WIDTH),
        Constraint::Min(0),
    ]).split(outer[1]);

    render_sidebar(f, app, columns[0]);
    render_code(f, app, columns[1]);

    // Status bar
    let mut hints = Vec::new();
    hints.push(Span::styled(" Tab switch  ", S::HINT_DEFAULT));
    hints.push(Span::styled("↑↓ scroll  ", S::HINT_DEFAULT));
    match app.focus {
        Focus::Code => {
            let jumpable = app.jump_targets[app.code_selected()].is_some();
            if jumpable {
                hints.push(Span::styled("→ jump  ", S::HINT_JUMP));
            }
            let depth = app.history.len();
            if depth > 0 {
                hints.push(Span::styled(format!("← back ({depth})  "), S::HINT_BACK));
            }
        }
        Focus::Sidebar => {
            if app.sidebar_current_is_jumpable() {
                hints.push(Span::styled("→/⏎ go to  ", S::HINT_JUMP));
            }
        }
    }
    hints.push(Span::styled("Esc quit", S::HINT_DEFAULT));
    let status = Paragraph::new(Line::from(hints)).style(S::BAR);
    f.render_widget(status, outer[2]);
}

fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Sidebar;
    let border_style = if focused { S::FN_LABEL } else { Style::new().fg(Color::DarkGray) };

    let tag_section_height = if app.tags.is_empty() { 0 } else { app.tags.len() as u16 + 2 };
    let sections = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(tag_section_height),
    ]).split(area);

    // --- Globals list ---
    let global_items: Vec<ListItem> = app.globals.iter().enumerate().map(|(i, (name, offset))| {
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {:>3}  ", i), S::ADDR),
            Span::styled(format!("{:<30}", name), if focused { S::FN_LABEL } else { Style::default() }),
            Span::styled(format!("0x{:04X}", offset), S::ADDR),
        ]))
    }).collect();

    let globals_block = Block::default()
        .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
        .border_style(border_style)
        .title(Span::styled(
            format!(" Globals ({}) ", app.globals.len()),
            if focused { S::FN_LABEL } else { Style::new().fg(Color::DarkGray) },
        ));

    if focused {
        let sel = app.sidebar_selected();
        if sel < app.globals.len() {
            let mut state = ListState::default();
            state.select(Some(sel));
            let list = List::new(global_items)
                .block(globals_block)
                .highlight_style(S::HIGHLIGHT);
            f.render_stateful_widget(list, sections[0], &mut state);
            // Write back scroll offset
            app.global_state = state;
        } else {
            let list = List::new(global_items).block(globals_block);
            f.render_widget(list, sections[0]);
        }
    } else {
        let list = List::new(global_items).block(globals_block);
        f.render_widget(list, sections[0]);
    }

    // --- Tags list ---
    if !app.tags.is_empty() {
        let tag_items: Vec<ListItem> = app.tags.iter().enumerate().map(|(i, name)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:>3}  ", i), S::ADDR),
                Span::styled(name.clone(), Style::new().fg(Color::Green)),
            ]))
        }).collect();

        let tags_block = Block::default()
            .borders(Borders::RIGHT | Borders::TOP)
            .border_style(border_style)
            .title(Span::styled(
                format!(" Tags ({}) ", app.tags.len()),
                if focused { S::FN_LABEL } else { Style::new().fg(Color::DarkGray) },
            ));

        if focused {
            let sel = app.sidebar_selected();
            if sel >= app.globals.len() + 1 {
                let tag_sel = sel - app.globals.len() - 1;
                let mut state = ListState::default();
                state.select(Some(tag_sel));
                let list = List::new(tag_items)
                    .block(tags_block)
                    .highlight_style(S::HIGHLIGHT);
                f.render_stateful_widget(list, sections[1], &mut state);
            } else {
                let list = List::new(tag_items).block(tags_block);
                f.render_widget(list, sections[1]);
            }
        } else {
            let list = List::new(tag_items).block(tags_block);
            f.render_widget(list, sections[1]);
        }
    }
}

fn render_code(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Code;
    let border_style = if focused { S::FN_LABEL } else { Style::new().fg(Color::DarkGray) };

    let code_block = Block::default()
        .borders(Borders::TOP)
        .border_style(border_style)
        .title(Span::styled(
            " Code ",
            if focused { S::FN_LABEL } else { Style::new().fg(Color::DarkGray) },
        ));

    let list_items: Vec<ListItem> = app.items.iter()
        .map(|line| ListItem::new(line.clone()))
        .collect();
    let list = List::new(list_items)
        .block(code_block)
        .highlight_style(S::HIGHLIGHT);
    f.render_stateful_widget(list, area, &mut app.code_state);
}
