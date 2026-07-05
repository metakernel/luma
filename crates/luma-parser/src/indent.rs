//! Indentation accounting for indentation-sensitive lexing.

use luma_syntax::{Diagnostic, DiagnosticCode, FileId, Span};

use crate::error::diagnostic;

/// Recorded indentation for a physical line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineIndent {
    /// One-based line number.
    pub line: usize,
    /// Leading spaces.
    pub width: usize,
    /// Leading indentation trivia span for the physical line.
    pub span: Span,
    /// Whether the line is blank or comment-only.
    pub is_ignorable: bool,
}

/// One active indentation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndentationFrame {
    /// Space count for this nesting level.
    pub width: usize,
}

/// State machine for indentation validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndentationState {
    stack: Vec<IndentationFrame>,
    previous_width: usize,
    previous_allows_child: bool,
}

impl Default for IndentationState {
    fn default() -> Self {
        Self {
            stack: vec![IndentationFrame { width: 0 }],
            previous_width: 0,
            previous_allows_child: false,
        }
    }
}

impl IndentationState {
    /// Validates one structural line.
    pub fn observe_line(
        &mut self,
        file_id: FileId,
        line_start: usize,
        indent_width: usize,
        allows_child: bool,
    ) -> Option<Diagnostic> {
        let current = self.stack.last().map_or(0, |frame| frame.width);

        let diagnostic = if indent_width > current {
            if self.previous_allows_child && indent_width > self.previous_width {
                self.stack.push(IndentationFrame {
                    width: indent_width,
                });
                None
            } else {
                Some(diagnostic(
                    DiagnosticCode::InvalidIndentation,
                    Some(Span::new(
                        file_id,
                        line_start,
                        line_start + indent_width.max(1),
                    )),
                ))
            }
        } else {
            while self
                .stack
                .last()
                .is_some_and(|frame| frame.width > indent_width)
            {
                self.stack.pop();
            }

            if self
                .stack
                .last()
                .is_some_and(|frame| frame.width == indent_width)
            {
                None
            } else {
                Some(diagnostic(
                    DiagnosticCode::InvalidIndentation,
                    Some(Span::new(
                        file_id,
                        line_start,
                        line_start + indent_width.max(1),
                    )),
                ))
            }
        };

        self.previous_width = indent_width;
        self.previous_allows_child = allows_child;
        diagnostic
    }
}

#[cfg(test)]
mod tests {
    use luma_syntax::{DiagnosticCode, FileId};

    use super::IndentationState;

    #[test]
    fn accepts_nested_then_sibling_dedent() {
        let mut state = IndentationState::default();
        assert!(state.observe_line(FileId(1), 0, 0, true).is_none());
        assert!(state.observe_line(FileId(1), 3, 2, false).is_none());
        assert!(state.observe_line(FileId(1), 8, 2, false).is_none());
        assert!(state.observe_line(FileId(1), 13, 0, false).is_none());
    }

    #[test]
    fn rejects_unexpected_indent() {
        let mut state = IndentationState::default();
        assert!(state.observe_line(FileId(1), 0, 0, false).is_none());
        let diagnostic = state.observe_line(FileId(1), 4, 2, false).unwrap();
        assert_eq!(diagnostic.code, DiagnosticCode::InvalidIndentation);
    }
}
