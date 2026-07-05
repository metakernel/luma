//! Directive parsing helpers.

use luma_syntax::{
    Diagnostic, DiagnosticCode, Directive, FileId, ImportDirective, IncludeDirective,
    LuaPreludeDirective, LumaProfile, MetaDirective, ProfileDirective, SchemaDirective, Span,
    StringNode, StringStyle, UseDirective, VersionDirective,
};

use crate::{
    error::{diagnostic, diagnostic_with_message},
    lua_capture::block_expression,
    scalar::parse_quoted_string,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DirectiveParse {
    Regular(Directive),
    Meta,
    Lua,
    If(String),
    ElseIf(String),
    Else,
    For { bindings: String, iterable: String },
}

pub(crate) fn parse_directive_line(
    text: &str,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DirectiveParse> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix('@')?;
    let (name, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(name, tail)| (name, tail.trim_start()));
    let span = Span::new(file_id, start, start + trimmed.len());
    match name.trim_end_matches(':') {
        "luma" => Some(DirectiveParse::Regular(Directive::Version(
            VersionDirective {
                version: tail.to_owned(),
                span,
            },
        ))),
        "profile" => Some(DirectiveParse::Regular(Directive::Profile(
            ProfileDirective {
                profile: match tail {
                    "data" => LumaProfile::Data,
                    "safe" => LumaProfile::Safe,
                    "trusted" => LumaProfile::Trusted,
                    other => LumaProfile::Custom(other.to_owned()),
                },
                span,
            },
        ))),
        "schema" => scalar_string_node(
            tail,
            start + trimmed.len() - tail.len(),
            file_id,
            diagnostics,
        )
        .map(|location| {
            DirectiveParse::Regular(Directive::Schema(SchemaDirective { location, span }))
        }),
        "import" => parse_import(
            tail,
            span,
            start + trimmed.len() - tail.len(),
            file_id,
            diagnostics,
        ),
        "include" => scalar_string_node(
            tail,
            start + trimmed.len() - tail.len(),
            file_id,
            diagnostics,
        )
        .map(|location| {
            DirectiveParse::Regular(Directive::Include(IncludeDirective { location, span }))
        }),
        "use" => parse_use(tail, span, diagnostics),
        "lua" if trimmed.ends_with(':') => Some(DirectiveParse::Lua),
        "meta" if trimmed.ends_with(':') => Some(DirectiveParse::Meta),
        "if" if trimmed.ends_with(':') => Some(DirectiveParse::If(
            tail.trim_end_matches(':').trim().to_owned(),
        )),
        "elseif" if trimmed.ends_with(':') => Some(DirectiveParse::ElseIf(
            tail.trim_end_matches(':').trim().to_owned(),
        )),
        "else" if tail == ":" || tail.is_empty() || trimmed == "@else:" => {
            Some(DirectiveParse::Else)
        }
        "for" if trimmed.ends_with(':') => {
            parse_for(tail.trim_end_matches(':').trim(), diagnostics, span)
        }
        _ => {
            diagnostics.push(diagnostic_with_message(
                DiagnosticCode::UnknownDirective,
                Some(span),
                format!("unknown directive `@{name}`"),
            ));
            None
        }
    }
}

pub(crate) fn make_lua_prelude(source: String, span: Span) -> Directive {
    Directive::LuaPrelude(LuaPreludeDirective {
        block: block_expression(
            source,
            span.start,
            span.end,
            span.file_id,
            luma_syntax::BlockKind::LuaChunk,
            luma_syntax::BlockChomping::Clip,
        ),
        span,
    })
}

#[allow(clippy::missing_const_for_fn)]
pub(crate) fn make_meta(value: luma_syntax::MappingBlock, span: Span) -> Directive {
    Directive::Meta(MetaDirective { value, span })
}

fn parse_import(
    tail: &str,
    span: Span,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DirectiveParse> {
    let (location, alias) = tail.split_once(" as ")?;
    let location = scalar_string_node(location.trim(), start, file_id, diagnostics)?;
    Some(DirectiveParse::Regular(Directive::Import(
        ImportDirective {
            location,
            alias: alias.trim().to_owned(),
            span,
        },
    )))
}

fn parse_use(tail: &str, span: Span, diagnostics: &mut Vec<Diagnostic>) -> Option<DirectiveParse> {
    let (module, alias) = tail.split_once(" as ").ok_or(()).ok()?;
    let alias = alias.trim();
    if alias.is_empty() {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDirectiveSyntax,
            Some(span),
        ));
        return None;
    }
    Some(DirectiveParse::Regular(Directive::Use(UseDirective {
        module: module.trim().to_owned(),
        alias: alias.to_owned(),
        span,
    })))
}

fn parse_for(tail: &str, diagnostics: &mut Vec<Diagnostic>, span: Span) -> Option<DirectiveParse> {
    let Some((bindings, iterable)) = tail.split_once(" in ") else {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDirectiveSyntax,
            Some(span),
        ));
        return None;
    };
    Some(DirectiveParse::For {
        bindings: bindings.trim().to_owned(),
        iterable: iterable.trim().to_owned(),
    })
}

fn scalar_string_node(
    text: &str,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<StringNode> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDirectiveSyntax,
            Some(Span::new(file_id, start, start)),
        ));
        return None;
    }
    if let Some((value, style)) = parse_quoted_string(trimmed, diagnostics, file_id, start) {
        return Some(StringNode {
            value,
            source: trimmed.to_owned(),
            style,
            block_kind: None,
            chomping: None,
            span: Span::new(file_id, start, start + trimmed.len()),
        });
    }
    Some(StringNode {
        value: trimmed.to_owned(),
        source: trimmed.to_owned(),
        style: StringStyle::Plain,
        block_kind: None,
        chomping: None,
        span: Span::new(file_id, start, start + trimmed.len()),
    })
}
