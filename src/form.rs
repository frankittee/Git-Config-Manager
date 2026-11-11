//! Inline (non-fullscreen) profile form rendered below the shell prompt.
//!
//! `Form` holds pure state and key handling so it can be unit-tested without a
//! terminal; `run` drives it with crossterm raw mode and a ratatui inline viewport.

use std::{io, time::Duration};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::profiles::Profile;

const ACCENT: Color = Color::Cyan;
const ERROR: Color = Color::Red;
const MUTED: Color = Color::DarkGray;
// 5 fields x (2 lines + spacer) + ssh hint + error + help.
const HEIGHT: u16 = 18;
// Aliases listed in the SSH host hint before collapsing into "+N more".
const MAX_LISTED_ALIASES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Name,
    Email,
    Signing,
    SigningKey,
    SshHost,
}

impl Field {
    fn title(self) -> &'static str {
        match self {
            Field::Name => "Git author name",
            Field::Email => "Git author email",
            Field::Signing => "Commit signing",
            Field::SigningKey => "Signing key",
            Field::SshHost => "SSH host alias",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Outcome {
    Continue,
    Submit,
    Cancel,
}

pub struct Form {
    name: String,
    email: String,
    signing: bool,
    signing_key: String,
    ssh_host: String,
    aliases: Vec<String>,
    focus: Field,
    error: Option<String>,
}

impl Form {
    pub fn new(initial: Profile, signing: bool, aliases: Vec<String>) -> Self {
        Self {
            name: initial.name,
            email: initial.email,
            signing,
            signing_key: initial.signing_key.unwrap_or_default(),
            ssh_host: initial.ssh_host.unwrap_or_default(),
            // Bare IP addresses are host entries, not aliases worth suggesting.
            aliases: aliases
                .into_iter()
                .filter(|alias| alias.parse::<std::net::IpAddr>().is_err())
                .collect(),
            focus: Field::Name,
            error: None,
        }
    }

    /// Fields in tab order; the signing key is skipped while signing is off.
    fn visible(&self) -> Vec<Field> {
        let mut fields = vec![Field::Name, Field::Email, Field::Signing];
        if self.signing {
            fields.push(Field::SigningKey);
        }
        fields.push(Field::SshHost);
        fields
    }

    fn text(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Email => &self.email,
            Field::SigningKey => &self.signing_key,
            Field::SshHost => &self.ssh_host,
            Field::Signing => "",
        }
    }

    fn text_mut(&mut self, field: Field) -> Option<&mut String> {
        match field {
            Field::Name => Some(&mut self.name),
            Field::Email => Some(&mut self.email),
            Field::SigningKey => Some(&mut self.signing_key),
            Field::SshHost => Some(&mut self.ssh_host),
            Field::Signing => None,
        }
    }

    fn validate(&self, field: Field) -> Option<String> {
        let value = self.text(field).trim();
        match field {
            Field::Name | Field::Email | Field::SigningKey if value.is_empty() => {
                Some(format!("{} must not be empty", field.title()))
            }
            Field::SshHost
                if value.chars().any(char::is_whitespace) || value.contains(['*', '?', '!']) =>
            {
                Some("SSH host alias must be literal and contain no whitespace".to_owned())
            }
            _ => None,
        }
    }

    fn move_focus(&mut self, step: isize) {
        let fields = self.visible();
        let index = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = index.saturating_add_signed(step).min(fields.len() - 1);
        self.focus = fields[next];
    }

    fn is_last(&self) -> bool {
        self.visible().last() == Some(&self.focus)
    }

    fn advance(&mut self) -> Outcome {
        if let Some(error) = self.validate(self.focus) {
            self.error = Some(error);
            return Outcome::Continue;
        }
        if !self.is_last() {
            self.move_focus(1);
            return Outcome::Continue;
        }
        // Earlier fields may have been edited after leaving them, so recheck all.
        if let Some(field) = self
            .visible()
            .into_iter()
            .find(|f| self.validate(*f).is_some())
        {
            self.focus = field;
            self.error = self.validate(field);
            return Outcome::Continue;
        }
        Outcome::Submit
    }

    fn complete_alias(&mut self) {
        let prefix = self.ssh_host.to_ascii_lowercase();
        if let Some(alias) = self
            .aliases
            .iter()
            .find(|a| a.to_ascii_lowercase().starts_with(&prefix))
        {
            self.ssh_host = alias.clone();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        self.error = None;
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('c') if ctrl => return Outcome::Cancel,
            KeyCode::Enter => return self.advance(),
            KeyCode::Tab if self.focus == Field::SshHost => self.complete_alias(),
            KeyCode::Tab | KeyCode::Down => return self.advance(),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(-1),
            _ if self.focus == Field::Signing => match key.code {
                KeyCode::Left | KeyCode::Char('h' | 'n' | 'N') => self.signing = false,
                KeyCode::Right | KeyCode::Char('l' | 'y' | 'Y') => self.signing = true,
                KeyCode::Char(' ') => self.signing = !self.signing,
                _ => {}
            },
            KeyCode::Backspace => {
                if let Some(text) = self.text_mut(self.focus) {
                    text.pop();
                }
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(text) = self.text_mut(self.focus) {
                    text.clear();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                if let Some(text) = self.text_mut(self.focus) {
                    text.push(c);
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    pub fn profile(&self) -> Profile {
        let ssh_host = self.ssh_host.trim();
        Profile {
            name: self.name.trim().to_owned(),
            email: self.email.trim().to_owned(),
            signing_key: self.signing.then(|| self.signing_key.trim().to_owned()),
            ssh_host: (!ssh_host.is_empty()).then(|| ssh_host.to_owned()),
        }
    }

    fn help(&self) -> String {
        let fields = self.visible();
        let mut parts = Vec::new();
        match self.focus {
            Field::Signing => parts.push("←/→ toggle"),
            Field::SshHost if !self.aliases.is_empty() => parts.push("tab complete"),
            _ => {}
        }
        parts.push(if self.is_last() {
            "enter submit"
        } else {
            "enter next"
        });
        if fields.first() != Some(&self.focus) {
            parts.push("shift+tab back");
        }
        parts.push("esc cancel");
        parts.join("  ")
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let muted = Style::default().fg(MUTED);
        let cursor = Style::default().add_modifier(Modifier::REVERSED);
        let mut lines = Vec::new();
        for field in self.visible() {
            let focused = field == self.focus;
            let (bar, title_style, text_style) = if focused {
                (
                    Span::styled("│ ", Style::default().fg(ACCENT)),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    Style::default(),
                )
            } else {
                (Span::raw("  "), muted.add_modifier(Modifier::BOLD), muted)
            };
            lines.push(Line::from(vec![
                bar.clone(),
                Span::styled(field.title(), title_style),
            ]));

            let prompt = Span::styled(
                "> ",
                if focused {
                    Style::default().fg(ACCENT)
                } else {
                    muted
                },
            );
            let mut spans = vec![bar, prompt];
            if field == Field::Signing {
                let (yes, no) = if self.signing {
                    (true, false)
                } else {
                    (false, true)
                };
                let pick = |selected: bool| match (selected, focused) {
                    (true, true) => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    (true, false) => Style::default(),
                    (false, _) => muted,
                };
                spans.push(Span::styled("Yes", pick(yes)));
                spans.push(Span::styled(" / ", muted));
                spans.push(Span::styled("No", pick(no)));
            } else {
                let value = self.text(field);
                spans.push(Span::styled(value.to_owned(), text_style));
                if focused {
                    spans.push(Span::styled(" ", cursor));
                }
                if value.is_empty() && field == Field::SshHost {
                    spans.push(Span::styled("leave blank to skip", muted));
                }
            }
            lines.push(Line::from(spans));

            if field == Field::SshHost && !self.aliases.is_empty() {
                lines.push(Line::styled(
                    format!("    available: {}", alias_hint(&self.aliases)),
                    muted,
                ));
            }
            if focused && let Some(error) = &self.error {
                lines.push(Line::styled(
                    format!("  ✗ {error}"),
                    Style::default().fg(ERROR),
                ));
            }
            lines.push(Line::raw(""));
        }
        lines.push(Line::styled(format!("  {}", self.help()), muted));
        lines
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let area = Rect {
            height: area.height.min(HEIGHT),
            ..area
        };
        frame.render_widget(Paragraph::new(self.lines()), area);
    }
}

fn alias_hint(aliases: &[String]) -> String {
    let mut hint = aliases
        .iter()
        .take(MAX_LISTED_ALIASES)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if aliases.len() > MAX_LISTED_ALIASES {
        hint.push_str(&format!(", +{} more", aliases.len() - MAX_LISTED_ALIASES));
    }
    hint
}

/// Restores the terminal on every exit path, including panics.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("could not enable terminal raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        execute!(io::stderr(), Show).ok();
        disable_raw_mode().ok();
    }
}

/// Runs the form on stderr so stdout stays free for scripts. The form area is
/// cleared before returning, leaving the cursor where the form started.
pub fn run(mut form: Form) -> Result<Profile> {
    let _guard = RawModeGuard::enter()?;
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(HEIGHT),
        },
    )
    .context("could not initialize terminal")?;

    let outcome = loop {
        terminal
            .draw(|frame| form.render(frame))
            .context("could not draw form")?;
        if !event::poll(Duration::from_millis(250)).context("could not poll terminal events")? {
            continue;
        }
        if let Event::Key(key) = event::read().context("could not read terminal event")?
            && key.kind == KeyEventKind::Press
        {
            match form.handle_key(key) {
                Outcome::Continue => {}
                done => break done,
            }
        }
    };
    terminal.clear().context("could not clear form")?;
    terminal.show_cursor().ok();
    match outcome {
        Outcome::Submit => Ok(form.profile()),
        _ => bail!("cancelled"),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(form: &mut Form, text: &str) {
        for c in text.chars() {
            form.handle_key(key(KeyCode::Char(c)));
        }
    }

    fn empty_form() -> Form {
        let blank = Profile {
            name: String::new(),
            email: String::new(),
            signing_key: None,
            ssh_host: None,
        };
        Form::new(
            blank,
            false,
            vec!["github-work".into(), "github-home".into()],
        )
    }

    #[test]
    fn required_fields_block_advancing() {
        let mut form = empty_form();
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::Name);
        assert!(form.error.as_deref().unwrap().contains("Git author name"));
        type_text(&mut form, "Ada");
        assert_eq!(form.error, None);
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::Email);
    }

    #[test]
    fn signing_off_skips_signing_key() {
        let mut form = empty_form();
        type_text(&mut form, "Ada");
        form.handle_key(key(KeyCode::Enter));
        type_text(&mut form, "ada@example.com");
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::Signing);
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::SshHost);
        form.handle_key(key(KeyCode::BackTab));
        assert_eq!(form.focus, Field::Signing);
    }

    #[test]
    fn signing_on_requires_key_and_toggle_keys_work() {
        let mut form = empty_form();
        form.focus = Field::Signing;
        form.handle_key(key(KeyCode::Char('y')));
        assert!(form.signing);
        form.handle_key(key(KeyCode::Char(' ')));
        assert!(!form.signing);
        form.handle_key(key(KeyCode::Right));
        assert!(form.signing);
        form.handle_key(key(KeyCode::Left));
        assert!(!form.signing);
        form.handle_key(key(KeyCode::Char('y')));
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::SigningKey);
        form.handle_key(key(KeyCode::Enter));
        assert!(form.error.is_some());
        type_text(&mut form, "KEY");
        form.handle_key(key(KeyCode::Enter));
        assert_eq!(form.focus, Field::SshHost);
    }

    #[test]
    fn submit_builds_trimmed_profile() {
        let mut form = empty_form();
        type_text(&mut form, " Ada ");
        form.handle_key(key(KeyCode::Enter));
        type_text(&mut form, "ada@example.com");
        form.handle_key(key(KeyCode::Enter));
        form.handle_key(key(KeyCode::Char('y')));
        form.handle_key(key(KeyCode::Enter));
        type_text(&mut form, "KEY");
        form.handle_key(key(KeyCode::Enter));
        type_text(&mut form, "github-w");
        form.handle_key(key(KeyCode::Tab));
        assert_eq!(form.handle_key(key(KeyCode::Enter)), Outcome::Submit);
        assert_eq!(
            form.profile(),
            Profile {
                name: "Ada".into(),
                email: "ada@example.com".into(),
                signing_key: Some("KEY".into()),
                ssh_host: Some("github-work".into()),
            }
        );
    }

    #[test]
    fn invalid_ssh_host_blocks_submit() {
        let mut form = empty_form();
        form.name = "Ada".into();
        form.email = "a@b.c".into();
        form.focus = Field::SshHost;
        type_text(&mut form, "bad host");
        assert_eq!(form.handle_key(key(KeyCode::Enter)), Outcome::Continue);
        assert!(form.error.as_deref().unwrap().contains("SSH host alias"));
    }

    #[test]
    fn prefill_and_back_navigation_revalidate_on_submit() {
        let initial = Profile {
            name: "Ada".into(),
            email: "a@b.c".into(),
            signing_key: Some("KEY".into()),
            ssh_host: Some("gh".into()),
        };
        let mut form = Form::new(initial.clone(), true, vec![]);
        assert_eq!(form.profile(), initial);
        form.focus = Field::SshHost;
        form.handle_key(key(KeyCode::BackTab));
        form.handle_key(key(KeyCode::BackTab));
        form.handle_key(key(KeyCode::BackTab));
        assert_eq!(form.focus, Field::Email);
        form.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        form.focus = Field::SshHost;
        assert_eq!(form.handle_key(key(KeyCode::Enter)), Outcome::Continue);
        assert_eq!(form.focus, Field::Email);
        assert!(form.error.is_some());
    }

    #[test]
    fn esc_and_ctrl_c_cancel() {
        let mut form = empty_form();
        assert_eq!(form.handle_key(key(KeyCode::Esc)), Outcome::Cancel);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(form.handle_key(ctrl_c), Outcome::Cancel);
    }

    #[test]
    fn renders_titles_prompt_hint_and_help() {
        let form = empty_form();
        let mut terminal = Terminal::new(TestBackend::new(60, HEIGHT)).unwrap();
        terminal.draw(|frame| form.render(frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        for expected in [
            "│ Git author name",
            "> ",
            "Yes / No",
            "leave blank to skip",
            "available: github-work, github-home",
            "enter next  esc cancel",
        ] {
            assert!(text.contains(expected), "missing {expected:?}");
        }
        assert!(!text.contains("Signing key"));
    }

    #[test]
    fn alias_hint_skips_ip_addresses_and_collapses_long_lists() {
        let aliases = ["10.0.0.1", "::1", "a", "b", "c", "d", "e", "f"]
            .map(String::from)
            .to_vec();
        let form = Form::new(empty_form().profile(), false, aliases);
        assert_eq!(form.aliases, ["a", "b", "c", "d", "e", "f"]);
        assert_eq!(alias_hint(&form.aliases), "a, b, c, d, +2 more");
        assert_eq!(alias_hint(&form.aliases[..2]), "a, b");
    }
}
