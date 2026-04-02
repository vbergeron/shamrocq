use ratatui::prelude::*;

pub const ADDR: Style = Style::new().fg(Color::DarkGray);
pub const COMMENT: Style = Style::new().fg(Color::Gray);
pub const FN_LABEL: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const BRANCH_LABEL: Style = Style::new().fg(Color::Yellow);
pub const MNEMONIC: Style = Style::new().add_modifier(Modifier::BOLD);
pub const MNEMONIC_BRANCH: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);
pub const MNEMONIC_JUMP: Style = Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD);
pub const OPERAND: Style = Style::new();
pub const MATCH_PIPE: Style = Style::new().fg(Color::Yellow);
pub const MATCH_ARITY: Style = Style::new().fg(Color::DarkGray);
pub const MATCH_TARGET: Style = Style::new();
pub const MATCH_TARGET_JUMP: Style = Style::new().fg(Color::Blue);
pub const HIGHLIGHT: Style = Style::new().bg(Color::Rgb(40, 40, 60));
pub const BAR: Style = Style::new().bg(Color::DarkGray).fg(Color::White);
pub const HINT_DEFAULT: Style = Style::new().fg(Color::White);
pub const HINT_JUMP: Style = Style::new().fg(Color::Green);
pub const HINT_BACK: Style = Style::new().fg(Color::Yellow);
