use crate::{SourceMap, Span};
use codespan_reporting::diagnostic::{Diagnostic as CsDiagnostic, Label as CsLabel};
use codespan_reporting::term;
use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Error,
    Warning,
    Note,
    Help,
}

/// An extra annotated span attached to a diagnostic.
#[derive(Clone, Debug)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
    /// Main location. `None` for errors that have no source position.
    pub primary: Option<Span>,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    pub fn new(level: Level, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            level,
            message: message.into(),
            primary: None,
            labels: Vec::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Diagnostic { Diagnostic::new(Level::Error, message) }

    pub fn warning(message: impl Into<String>) -> Diagnostic { Diagnostic::new(Level::Warning, message) }

    pub fn with_span(mut self, span: Span) -> Diagnostic {
        self.primary = Some(span);
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    fn to_codespan(&self) -> CsDiagnostic<usize> {
        let base = match self.level {
            Level::Error => CsDiagnostic::error(),
            Level::Warning => CsDiagnostic::warning(),
            Level::Note => CsDiagnostic::note(),
            Level::Help => CsDiagnostic::help(),
        };
        let mut labels = Vec::new();
        if let Some(span) = self.primary {
            labels.push(CsLabel::primary(span.file.into(), span.range()));
        }
        for l in &self.labels {
            labels.push(CsLabel::secondary(l.span.file.into(), l.span.range()).with_message(l.message.clone()));
        }
        base.with_message(self.message.clone()).with_labels(labels)
    }
}

#[derive(Default)]
pub struct DiagnosticsCtx {
    diagnostics: RefCell<Vec<Diagnostic>>,
}

impl DiagnosticsCtx {
    pub fn new() -> DiagnosticsCtx { DiagnosticsCtx::default() }

    pub fn emit(&self, diag: Diagnostic) { self.diagnostics.borrow_mut().push(diag); }

    pub fn error(&self, span: Span, message: impl Into<String>) { self.emit(Diagnostic::error(message).with_span(span)); }

    pub fn has_errors(&self) -> bool { self.diagnostics.borrow().iter().any(|d| d.level == Level::Error) }

    pub fn error_count(&self) -> usize { self.diagnostics.borrow().iter().filter(|d| d.level == Level::Error).count() }

    pub fn diagnostics(&self) -> Vec<Diagnostic> { self.diagnostics.borrow().clone() }

    pub fn format(&self, map: &SourceMap) -> String {
        let config = term::Config::default();
        let mut out = String::new();
        for d in self.diagnostics.borrow().iter() {
            match term::emit_into_string(&config, map.codespan_files(), &d.to_codespan()) {
                Ok(s) => out.push_str(&s),
                Err(_) => {
                    out.push_str(&format!("{:?}: {}\n", d.level, d.message));
                }
            }
        }
        out
    }
}
