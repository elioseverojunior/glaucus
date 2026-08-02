// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One diagnostic type and one renderer, so exactly one place decides what a
//! problem looks like on screen.

use std::fmt::Write as _;
use std::io::Write;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

/// A tab renders as this many spaces, in both the echoed line and the caret row.
const TAB_WIDTH: usize = 4;
/// Maximum display width of an echoed source line before it is windowed.
const MAX_LINE_WIDTH: usize = 120;

/// How serious a report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A failure.
    Error,
    /// Something suspicious that did not stop the run.
    Warning,
    /// Additional context.
    Note,
}

impl Severity {
    /// The word shown at the head of the report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    /// The ANSI SGR parameters used when colour is on.
    const fn sgr(self) -> &'static str {
        match self {
            Self::Error => "1;31",
            Self::Warning => "1;33",
            Self::Note => "1;36",
        }
    }
}

/// A single problem, normalised from any source.
#[derive(Debug, Clone)]
pub struct Report {
    /// How serious it is.
    pub severity: Severity,
    /// One-line description.
    pub message: String,
    /// Source file, when the input was not stdin.
    pub file: Option<PathBuf>,
    /// 1-based line. `0` means unknown.
    pub line: u32,
    /// 1-based column **in bytes**. `0` means unknown.
    pub column: u32,
    /// JSON-pointer-ish path to the offending node.
    pub path: Option<String>,
    /// Suggested next step.
    pub help: Option<String>,
}

impl Report {
    /// Starts a report. Chain the optional parts, then [`ReportBuilder::build`].
    #[must_use]
    pub fn builder(severity: Severity, message: impl Into<String>) -> ReportBuilder {
        ReportBuilder {
            report: Self {
                severity,
                message: message.into(),
                file: None,
                line: 0,
                column: 0,
                path: None,
                help: None,
            },
        }
    }
}

/// Fluent constructor for [`Report`].
///
/// Seven fields, most of them optional, built at many call sites — exactly the
/// shape a wide constructor or a bare struct literal handles badly. Struct
/// literals for `Report` are confined to this module.
#[derive(Debug, Clone)]
pub struct ReportBuilder {
    report: Report,
}

impl ReportBuilder {
    /// Sets the source file. `None` means stdin.
    #[must_use]
    pub fn file(mut self, file: Option<PathBuf>) -> Self {
        self.report.file = file;
        self
    }

    /// Sets the 1-based line and 1-based BYTE column. Zero means unknown.
    #[must_use]
    pub const fn location(mut self, line: u32, column: u32) -> Self {
        self.report.line = line;
        self.report.column = column;
        self
    }

    /// Sets the JSON-pointer path to the offending node.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.report.path = Some(path.into());
        self
    }

    /// Sets the suggested next step.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.report.help = Some(help.into());
        self
    }

    /// Finishes the report.
    #[must_use]
    pub fn build(self) -> Report {
        self.report
    }
}

/// Rendering policy.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Emit ANSI colour.
    pub color: bool,
    /// Echo the offending source line. Off suppresses possible secrets.
    pub show_source: bool,
}

/// Renders `report` in human form.
///
/// # Errors
///
/// Propagates write failures from `out`.
pub fn render(
    report: &Report,
    source: Option<&str>,
    options: RenderOptions,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    write_headline(report, options, out)?;
    if report.line == 0 {
        return Ok(());
    }
    write_location(report, out)?;

    let Some(text) = source.filter(|_| options.show_source) else {
        return Ok(());
    };
    let Some(raw) = text.lines().nth(report.line as usize - 1) else {
        return Ok(());
    };
    write_snippet(report, raw, out)
}

/// Writes the `severity: message` line, coloured when requested.
fn write_headline(
    report: &Report,
    options: RenderOptions,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let label = report.severity.label();
    if options.color {
        writeln!(
            out,
            "\u{1b}[{}m{label}\u{1b}[0m: {}",
            report.severity.sgr(),
            report.message
        )
    } else {
        writeln!(out, "{label}: {}", report.message)
    }
}

/// Writes the `  --> file:line:column` line.
fn write_location(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let name = report
        .file
        .as_ref()
        .map_or_else(|| "<stdin>".to_string(), |file| file.display().to_string());
    writeln!(out, "  --> {name}:{}:{}", report.line, report.column)
}

/// Writes the echoed source line, its caret row, and the optional help line.
fn write_snippet(report: &Report, raw: &str, out: &mut dyn Write) -> std::io::Result<()> {
    let gutter = report.line.to_string();
    let pad = " ".repeat(gutter.len().max(2));
    let (shown, caret_col) = window(raw, report.column);

    writeln!(out, "{pad} |")?;
    writeln!(out, "{gutter} | {shown}")?;
    write_caret(report, &pad, caret_col, out)?;

    if let Some(help) = &report.help {
        writeln!(out, "{pad} |")?;
        writeln!(out, "{pad} = help: {help}")?;
    }
    Ok(())
}

/// Writes the `^` caret row, with the offending path appended when known.
fn write_caret(
    report: &Report,
    pad: &str,
    caret_col: usize,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let mut caret = format!("{pad} | {}^", " ".repeat(caret_col));
    if let Some(path) = &report.path {
        let _ = write!(caret, " at {path}");
    }
    writeln!(out, "{caret}")
}

/// Expands tabs, windows over-long lines, and returns the display column the
/// caret belongs at.
///
/// `column` is 1-based **bytes**, matching `Position::column`, so the prefix is
/// sliced by byte index and then measured for display width.
fn window(raw: &str, column: u32) -> (String, usize) {
    let byte_col = (column as usize).saturating_sub(1).min(raw.len());
    let prefix = raw.get(..byte_col).unwrap_or(raw);

    let expanded: String = raw.replace('\t', &" ".repeat(TAB_WIDTH));
    let caret_col = prefix.replace('\t', &" ".repeat(TAB_WIDTH)).width();

    if expanded.width() <= MAX_LINE_WIDTH {
        return (expanded, caret_col);
    }
    // Window around the caret so the interesting part stays visible.
    let start = caret_col.saturating_sub(MAX_LINE_WIDTH / 2);
    let shown: String = expanded.chars().skip(start).take(MAX_LINE_WIDTH).collect();
    (format!("...{shown}..."), caret_col - start + 3)
}

/// Renders `report` as one JSON object followed by a newline.
///
/// # Errors
///
/// Propagates write failures from `out`.
pub fn render_json(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let value = serde_json::json!({
        "severity": report.severity.label(),
        "message": report.message,
        "file": report.file.as_ref().map(|file| file.display().to_string()),
        "line": report.line,
        "column": report.column,
        "path": report.path,
        "help": report.help,
    });
    writeln!(out, "{value}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report::builder(Severity::Error, "expected integer, found string")
            .file(Some("deploy.yaml".into()))
            .location(1, 9)
            .path(".spec.port")
            .help("run with --fix to coerce")
            .build()
    }

    #[test]
    fn builder_leaves_unset_fields_empty() {
        let report = Report::builder(Severity::Warning, "m").build();
        assert_eq!(report.severity, Severity::Warning);
        assert_eq!(report.message, "m");
        assert!(report.file.is_none());
        assert_eq!(report.line, 0);
        assert_eq!(report.column, 0);
        assert!(report.path.is_none());
        assert!(report.help.is_none());
    }

    #[test]
    fn builder_chains_every_field() {
        let report = Report::builder(Severity::Error, "boom")
            .file(Some("a.yaml".into()))
            .location(3, 7)
            .path(".spec")
            .help("try --fix")
            .build();
        assert_eq!(report.file, Some("a.yaml".into()));
        assert_eq!(report.line, 3);
        assert_eq!(report.column, 7);
        assert_eq!(report.path.as_deref(), Some(".spec"));
        assert_eq!(report.help.as_deref(), Some("try --fix"));
    }

    fn options(show_source: bool) -> RenderOptions {
        RenderOptions {
            color: false,
            show_source,
        }
    }

    #[test]
    fn renders_caret_under_the_byte_column() {
        let src = "  port: \"8080\"\n";
        let mut out = Vec::new();
        render(&report(), Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("error: expected integer, found string"));
        assert!(s.contains("--> deploy.yaml:1:9"));
        // column 9 is 1-based bytes -> 8 columns of padding before the caret.
        assert!(s.contains("\n   |         ^"), "caret misaligned:\n{s}");
        assert!(s.contains("at .spec.port"));
        assert!(s.contains("= help: run with --fix to coerce"));
    }

    #[test]
    fn caret_aligns_under_wide_characters() {
        // Each CJK char is 3 bytes but 2 display columns. Byte column 7 is the
        // 3rd char, so display padding must be 4, not 6.
        let src = "名前: x\n";
        let mut r = report();
        r.column = 7;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(
            s.contains("\n   |     ^"),
            "wide-char caret misaligned:\n{s}"
        );
    }

    #[test]
    fn tabs_expand_consistently_in_line_and_caret() {
        let src = "\tport: 1\n";
        let mut r = report();
        r.column = 2; // just after the tab
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("    port: 1"), "tab not expanded:\n{s}");
        assert!(s.contains("\n   |     ^"), "caret ignores tab width:\n{s}");
    }

    #[test]
    fn no_source_suppresses_the_echo_but_keeps_location() {
        let src = "  password: hunter2\n";
        let mut out = Vec::new();
        render(&report(), Some(src), options(false), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("--> deploy.yaml:1:9"));
        assert!(!s.contains("hunter2"), "secret leaked:\n{s}");
    }

    #[test]
    fn long_lines_are_windowed() {
        let src = format!("{}port: 1\n", "x".repeat(400));
        let mut r = report();
        r.column = 401;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, Some(&src), options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("..."), "long line not windowed:\n{s}");
        assert!(
            s.lines().all(|l| l.chars().count() <= 160),
            "line too long:\n{s}"
        );
    }

    #[test]
    fn missing_source_renders_header_only() {
        let mut out = Vec::new();
        render(&report(), None, options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("--> deploy.yaml:1:9"));
        assert!(!s.contains(" | "));
    }

    #[test]
    fn zero_line_means_unknown_location() {
        let mut r = report();
        r.line = 0;
        r.column = 0;
        r.file = None;
        r.path = None;
        r.help = None;
        let mut out = Vec::new();
        render(&r, None, options(true), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "error: expected integer, found string\n");
    }

    #[test]
    fn severity_words_are_distinct() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Note.label(), "note");
    }

    #[test]
    fn colour_wraps_the_severity_label() {
        let mut out = Vec::new();
        render(
            &report(),
            None,
            RenderOptions {
                color: true,
                show_source: false,
            },
            &mut out,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\u{1b}["), "expected ANSI escape:\n{s}");
    }

    #[test]
    fn warning_severity_uses_yellow_ansi_code_when_coloured() {
        let mut report = report();
        report.severity = Severity::Warning;
        let mut out = Vec::new();
        render(
            &report,
            None,
            RenderOptions {
                color: true,
                show_source: false,
            },
            &mut out,
        )
        .unwrap();
        let rendered = String::from_utf8(out).unwrap();
        // The reset escape sits between the label and the colon (`warning\x1b[0m: `),
        // so this checks the SGR code directly wraps the "warning" label.
        assert!(
            rendered.contains("1;33mwarning\u{1b}[0m"),
            "expected yellow-wrapped warning label:\n{rendered}"
        );
    }

    #[test]
    fn note_severity_uses_cyan_ansi_code_when_coloured() {
        let mut report = report();
        report.severity = Severity::Note;
        let mut out = Vec::new();
        render(
            &report,
            None,
            RenderOptions {
                color: true,
                show_source: false,
            },
            &mut out,
        )
        .unwrap();
        let rendered = String::from_utf8(out).unwrap();
        assert!(
            rendered.contains("1;36mnote\u{1b}[0m"),
            "expected cyan-wrapped note label:\n{rendered}"
        );
    }

    #[test]
    fn line_beyond_source_length_skips_snippet_but_keeps_location() {
        let src = "first\nsecond\n";
        let mut report = report();
        report.line = 99;
        report.path = None;
        report.help = None;
        let mut out = Vec::new();
        render(&report, Some(src), options(true), &mut out).unwrap();
        let rendered = String::from_utf8(out).unwrap();
        assert!(
            rendered.contains("--> deploy.yaml:99:9"),
            "location line missing:\n{rendered}"
        );
        assert!(
            !rendered.contains(" | "),
            "snippet gutter should not print past end of file:\n{rendered}"
        );
    }

    #[test]
    fn json_render_emits_one_object_per_line() {
        let mut out = Vec::new();
        render_json(&report(), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["severity"], "error");
        assert_eq!(v["line"], 1);
        assert_eq!(v["column"], 9);
        assert_eq!(v["file"], "deploy.yaml");
        assert_eq!(v["path"], ".spec.port");
    }
}
