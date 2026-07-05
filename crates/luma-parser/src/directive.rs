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
    For {
        bindings: String,
        bindings_span: Span,
        iterable: String,
        iterable_start: usize,
    },
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
    let name_text = name.trim_end_matches(':');
    let name_span = Span::new(file_id, start + 1, start + 1 + name_text.len());
    match name_text {
        "luma" => Some(DirectiveParse::Regular(Directive::Version(
            VersionDirective {
                version: tail.to_owned(),
                name_span,
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
                name_span,
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
            DirectiveParse::Regular(Directive::Schema(SchemaDirective {
                location,
                name_span,
                span,
            }))
        }),
        "import" => parse_import(
            tail,
            name_span,
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
            DirectiveParse::Regular(Directive::Include(IncludeDirective {
                location,
                name_span,
                span,
            }))
        }),
        "use" => parse_use(
            tail,
            name_span,
            span,
            start + trimmed.len() - tail.len(),
            file_id,
            diagnostics,
        ),
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
        "for" if trimmed.ends_with(':') => parse_for(
            tail.trim_end_matches(':').trim(),
            diagnostics,
            span,
            start + trimmed.len() - tail.len(),
            file_id,
        ),
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
        name_span: Span::new(span.file_id, span.start + 1, span.start + 4),
        span,
    })
}

#[allow(clippy::missing_const_for_fn)]
pub(crate) fn make_meta(value: luma_syntax::MappingBlock, span: Span) -> Directive {
    Directive::Meta(MetaDirective {
        value,
        name_span: Span::new(span.file_id, span.start + 1, span.start + 5),
        span,
    })
}

fn parse_import(
    tail: &str,
    name_span: Span,
    span: Span,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DirectiveParse> {
    let as_index = tail.find(" as ")?;
    let (location, alias) = tail.split_at(as_index);
    let alias = &alias[4..];
    let location = scalar_string_node(location.trim(), start, file_id, diagnostics)?;
    let (alias, alias_start) = trimmed_segment(alias, start + as_index + 4);
    Some(DirectiveParse::Regular(Directive::Import(
        ImportDirective {
            location,
            alias: alias.to_owned(),
            name_span,
            alias_span: Span::new(file_id, alias_start, alias_start + alias.len()),
            span,
        },
    )))
}

fn parse_use(
    tail: &str,
    name_span: Span,
    span: Span,
    start: usize,
    file_id: FileId,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DirectiveParse> {
    let as_index = tail.find(" as ").ok_or(()).ok()?;
    let (module, alias) = tail.split_at(as_index);
    let alias = &alias[4..];
    let (module, module_start) = trimmed_segment(module, start);
    let (alias, alias_start) = trimmed_segment(alias, start + as_index + 4);
    if alias.is_empty() {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDirectiveSyntax,
            Some(span),
        ));
        return None;
    }
    Some(DirectiveParse::Regular(Directive::Use(UseDirective {
        module: module.to_owned(),
        alias: alias.to_owned(),
        name_span,
        module_span: Span::new(file_id, module_start, module_start + module.len()),
        alias_span: Span::new(file_id, alias_start, alias_start + alias.len()),
        span,
    })))
}

fn parse_for(
    tail: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Span,
    start: usize,
    file_id: FileId,
) -> Option<DirectiveParse> {
    let Some(in_index) = tail.find(" in ") else {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDirectiveSyntax,
            Some(span),
        ));
        return None;
    };
    let (bindings, iterable) = tail.split_at(in_index);
    let iterable = &iterable[4..];
    let (bindings, bindings_start) = trimmed_segment(bindings, start);
    let (iterable, iterable_start) = trimmed_segment(iterable, start + in_index + 4);
    Some(DirectiveParse::For {
        bindings: bindings.to_owned(),
        bindings_span: Span::new(file_id, bindings_start, bindings_start + bindings.len()),
        iterable: iterable.to_owned(),
        iterable_start,
    })
}

fn trimmed_segment(text: &str, start: usize) -> (&str, usize) {
    let trimmed = text.trim();
    let leading = text.find(trimmed).unwrap_or(0);
    (trimmed, start + leading)
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
