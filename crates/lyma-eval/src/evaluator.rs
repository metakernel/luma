//! Engine-agnostic AST evaluator.

use lyma_runtime::{LuaRuntimeEngine, LuaSourceText, RuntimeEnvironment, RuntimeModule};
use lyma_syntax::{
    Directive, Document, DocumentItem, LoopBindings, LymaKey, LymaMapping, LymaMappingEntry,
    LymaNode, LymaNull, LymaNumber, LymaProfile, LymaSequence, LymaTaggedValue, LymaValue,
    MappingItem, MappingKey, SequenceItem,
};

use crate::{
    DocumentMetadata, EvaluatedDocument, EvaluationOptions, ModuleLookupRequest, ResolutionContext,
    ResolutionKind, TagResolutionRequest, UnknownTagPolicy,
    context::{EvaluationError, ResourceContext},
    control::{is_truthy, iter_items},
    imports::load_lyma_resource,
    runtime_values::{stabilize_lyma_value, stabilize_runtime_value},
    schema_validator::validate_document_schema,
    scope::{RuntimeScope, path_segment_from_index, path_segment_from_key},
    spread::{spread_mapping, spread_sequence},
};

/// Concrete evaluator for parsed Lyma documents.
pub struct AstEvaluator<'a, E: LuaRuntimeEngine> {
    /// Runtime engine.
    pub engine: &'a E,
    /// Evaluation options.
    pub options: EvaluationOptions<'a, E>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvaluationMode {
    Document,
    Schema,
}

#[derive(Debug, Clone)]
struct DocumentState {
    metadata: DocumentMetadata,
    data_only: bool,
    reject_unknown_tags: bool,
}

impl<E: LuaRuntimeEngine> AstEvaluator<'_, E> {
    /// Evaluates the first root-bearing document in `file`.
    ///
    /// # Errors
    ///
    /// Returns an error when document evaluation, schema validation, runtime
    /// execution, imports/includes, or profile output validation fails.
    pub fn evaluate_file(
        &self,
        file: &lyma_syntax::LymaFile,
        source_name: &str,
        locator: Option<crate::ResourceLocator>,
    ) -> Result<Vec<LymaValue>, EvaluationError> {
        Ok(self
            .evaluate_file_with_metadata(file, source_name, locator)?
            .into_iter()
            .map(|document| document.value)
            .collect())
    }

    /// Evaluates documents while extracting document metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when document evaluation, schema validation, runtime
    /// execution, imports/includes, or profile output validation fails.
    pub fn evaluate_file_with_metadata(
        &self,
        file: &lyma_syntax::LymaFile,
        source_name: &str,
        locator: Option<crate::ResourceLocator>,
    ) -> Result<Vec<EvaluatedDocument>, EvaluationError> {
        let resource = ResourceContext::new(source_name, locator);
        file.documents
            .iter()
            .map(|document| {
                let mut resolver_context = ResolutionContext::new(
                    self.options
                        .profile
                        .profile()
                        .runtime_limits
                        .max_table_entries
                        .unwrap_or(1024),
                );
                self.evaluate_document(
                    document,
                    &resource,
                    &mut resolver_context,
                    EvaluationMode::Document,
                )
            })
            .collect()
    }

    pub(crate) fn evaluate_schema_document(
        &self,
        document: &Document,
        resource: &ResourceContext,
        resolver_context: &mut ResolutionContext,
    ) -> Result<EvaluatedDocument, EvaluationError> {
        self.evaluate_document(document, resource, resolver_context, EvaluationMode::Schema)
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_document(
        &self,
        document: &Document,
        resource: &ResourceContext,
        resolver_context: &mut ResolutionContext,
        mode: EvaluationMode,
    ) -> Result<EvaluatedDocument, EvaluationError> {
        let mut scope = RuntimeScope::new(
            self.engine.create_environment()?,
            resource.locator.clone(),
            resource.root.clone(),
        );
        scope = scope.child(self.engine, None, &LymaValue::Null(LymaNull), None, None)?;

        let mut state = self.plan_document_state(document)?;
        let mut root_value: Option<LymaValue> = None;

        for item in &document.items {
            match item {
                DocumentItem::Directive(directive) => match directive {
                    Directive::Schema(_) | Directive::Version(_) | Directive::Profile(_) => {}
                    Directive::LuaPrelude(value) => {
                        Self::reject_unsafe_source(&value.block.source)?;
                        let compiled = self.engine.compile_chunk(
                            LuaSourceText::new(resource.source_name.as_str(), &value.block.source)
                                .with_span(value.block.span),
                            &self.options.profile.profile().runtime_limits,
                        )?;
                        let _ = self.engine.evaluate_chunk(
                            &compiled,
                            &mut scope.environment,
                            &self.options.profile.profile().runtime_limits,
                        )?;
                    }
                    Directive::Import(value) => {
                        let resolver = self.options.require_resolver("imports")?;
                        let (locator, file, _) = load_lyma_resource(
                            resolver,
                            ResolutionKind::Import,
                            &value.location.value,
                            resource.locator.as_ref(),
                            resolver_context,
                        )?;
                        let child_resource = resource.child(value.location.value.clone(), locator);
                        let imported = self
                            .evaluate_document(
                                &file.documents[0],
                                &child_resource,
                                resolver_context,
                                mode,
                            )?
                            .value;
                        scope.bind_value(self.engine, &value.alias, &imported)?;
                    }
                    Directive::Include(value) => {
                        let resolver = self.options.require_resolver("includes")?;
                        let (locator, file, _) = load_lyma_resource(
                            resolver,
                            ResolutionKind::Include,
                            &value.location.value,
                            resource.locator.as_ref(),
                            resolver_context,
                        )?;
                        let child_resource = resource.child(value.location.value.clone(), locator);
                        let included = self
                            .evaluate_document(
                                &file.documents[0],
                                &child_resource,
                                resolver_context,
                                mode,
                            )?
                            .value;
                        root_value = Some(match root_value.take() {
                            None => included,
                            Some(existing) => Self::merge_top_level(existing, included)?,
                        });
                    }
                    Directive::Use(value) => {
                        let registry = self.options.require_module_registry()?;
                        let module = registry.lookup_module(
                            self.engine,
                            ModuleLookupRequest {
                                specifier: &value.module,
                                from: resource.locator.as_ref(),
                                context: resolver_context,
                            },
                        )?;
                        let exports = module.exports().map_err(EvaluationError::from)?;
                        scope.environment.inject_module(module)?;
                        let exports_value = LymaValue::Mapping(LymaMapping {
                            entries: exports
                                .into_iter()
                                .map(|(name, value)| {
                                    Ok(LymaMappingEntry {
                                        key: LymaKey::String(name),
                                        value: stabilize_runtime_value(
                                            self.engine,
                                            &value,
                                            self.options.profile.profile(),
                                            None,
                                            state.data_only,
                                        )?,
                                        span: None,
                                    })
                                })
                                .collect::<Result<Vec<_>, EvaluationError>>()?,
                            duplicate_keys: Vec::new(),
                            span: None,
                        });
                        scope.bind_value(self.engine, &value.alias, &exports_value)?;
                    }
                    Directive::Meta(value) => {
                        let metadata_value = self.evaluate_mapping(
                            &value.value,
                            &scope,
                            None,
                            root_value.as_ref(),
                            resolver_context,
                            &state,
                        )?;
                        state.metadata.value = Some(match state.metadata.value.take() {
                            Some(existing) => Self::merge_top_level(existing, metadata_value)?,
                            None => metadata_value,
                        });
                    }
                },
                DocumentItem::Let(binding) => {
                    let value = self.evaluate_node(
                        &binding.value,
                        &scope,
                        None,
                        root_value.as_ref(),
                        resolver_context,
                        &state,
                    )?;
                    scope.bind_value(self.engine, &binding.name, &value)?;
                }
                DocumentItem::Root(node) => {
                    let value = self.evaluate_node(
                        node,
                        &scope,
                        None,
                        root_value.as_ref(),
                        resolver_context,
                        &state,
                    )?;
                    root_value = Some(match root_value.take() {
                        None => value,
                        Some(existing) => Self::merge_top_level(existing, value)?,
                    });
                }
                DocumentItem::Comment(_) => {}
            }
        }

        let value = root_value.unwrap_or(LymaValue::Null(LymaNull));
        if mode == EvaluationMode::Document {
            if let Some(schema) = state.metadata.schema.clone() {
                validate_document_schema(self, &schema, &value, resource, resolver_context)?;
            }
            self.options
                .profile
                .validate_runtime_output(&value)
                .map_err(|value| EvaluationError {
                    diagnostic: value.diagnostic,
                })?;
        }

        Ok(EvaluatedDocument {
            value: stabilize_lyma_value(&value, state.data_only)?,
            metadata: state.metadata,
        })
    }

    fn evaluate_node(
        &self,
        node: &LymaNode,
        scope: &RuntimeScope<E>,
        parent: Option<&LymaValue>,
        root: Option<&LymaValue>,
        resolver_context: &mut ResolutionContext,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        match node {
            LymaNode::Null { .. } => Ok(LymaValue::Null(LymaNull)),
            LymaNode::Boolean { value, .. } => Ok(LymaValue::Boolean(*value)),
            LymaNode::Number(number) => parse_number(&number.lexeme),
            LymaNode::String(string) => Ok(LymaValue::String(string.value.clone())),
            LymaNode::LuaExpression(expr)
            | LymaNode::LuaExpressionBlock(expr)
            | LymaNode::LuaTableConstructor(expr) => {
                self.evaluate_expression(scope, &expr.source, expr.span, state)
            }
            LymaNode::LuaChunk(expr) => self.evaluate_chunk(scope, &expr.source, expr.span, state),
            LymaNode::Tagged(tagged) => {
                let value = if let Some(value) = &tagged.value {
                    self.evaluate_node(value, scope, parent, root, resolver_context, state)?
                } else {
                    LymaValue::Null(LymaNull)
                };
                self.resolve_tag_value(tagged, &value, state)
            }
            LymaNode::Sequence(sequence) => {
                self.evaluate_sequence(sequence, scope, parent, root, resolver_context, state)
            }
            LymaNode::Mapping(mapping) => {
                self.evaluate_mapping(mapping, scope, parent, root, resolver_context, state)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_sequence(
        &self,
        sequence: &lyma_syntax::SequenceBlock,
        scope: &RuntimeScope<E>,
        parent: Option<&LymaValue>,
        root: Option<&LymaValue>,
        resolver_context: &mut ResolutionContext,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        let mut items = Vec::new();
        for item in &sequence.items {
            let current = LymaValue::Sequence(LymaSequence {
                items: items.clone(),
                span: Some(sequence.span),
            });
            let effective_root = root.unwrap_or(&current);
            match item {
                SequenceItem::Value(value) => {
                    let child = scope.child(
                        self.engine,
                        Some(path_segment_from_index(items.len())?),
                        &current,
                        parent,
                        Some(effective_root),
                    )?;
                    items.push(self.evaluate_node(
                        value,
                        &child,
                        Some(&current),
                        Some(effective_root),
                        resolver_context,
                        state,
                    )?);
                }
                SequenceItem::Spread(spread) => {
                    let child =
                        scope.child(self.engine, None, &current, parent, Some(effective_root))?;
                    let value = self.evaluate_expression(
                        &child,
                        &spread.expression.source,
                        spread.expression.span,
                        state,
                    )?;
                    spread_sequence(&mut items, value)?;
                }
                SequenceItem::Conditional(block) => {
                    if let Some(selected) = self.select_sequence_branch(
                        block,
                        scope,
                        &current,
                        Some(effective_root),
                        state,
                    )? {
                        let LymaValue::Sequence(selected) = self.evaluate_sequence(
                            &selected,
                            scope,
                            Some(&current),
                            Some(effective_root),
                            resolver_context,
                            state,
                        )?
                        else {
                            unreachable!()
                        };
                        items.extend(selected.items);
                    }
                }
                SequenceItem::Loop(block) => {
                    let iterated = self.evaluate_expression(
                        scope,
                        &block.iterable.source,
                        block.iterable.span,
                        state,
                    )?;
                    for loop_item in iter_items(&iterated)? {
                        let child = scope.child(
                            self.engine,
                            None,
                            &current,
                            parent,
                            Some(effective_root),
                        )?;
                        let mut child = child;
                        bind_loop_values(self.engine, &mut child, &block.bindings, &loop_item)?;
                        let LymaValue::Sequence(expanded) = self.evaluate_sequence(
                            &block.body,
                            &child,
                            Some(&current),
                            Some(effective_root),
                            resolver_context,
                            state,
                        )?
                        else {
                            unreachable!()
                        };
                        items.extend(expanded.items);
                    }
                }
                SequenceItem::Directive(Directive::Include(include)) => {
                    let resolver = self.options.require_resolver("includes")?;
                    let (locator, file, _) = load_lyma_resource(
                        resolver,
                        ResolutionKind::Include,
                        &include.location.value,
                        scope.locator.as_ref(),
                        resolver_context,
                    )?;
                    let value = self
                        .evaluate_document(
                            &file.documents[0],
                            &ResourceContext {
                                locator: Some(locator.clone()),
                                root: scope.root_locator.clone().or(Some(locator)),
                                source_name: include.location.value.clone(),
                            },
                            resolver_context,
                            EvaluationMode::Document,
                        )?
                        .value;
                    spread_sequence(&mut items, value)?;
                }
                SequenceItem::Directive(_) | SequenceItem::Comment(_) => {}
            }
        }
        Ok(LymaValue::Sequence(LymaSequence {
            items,
            span: Some(sequence.span),
        }))
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_mapping(
        &self,
        mapping: &lyma_syntax::MappingBlock,
        scope: &RuntimeScope<E>,
        parent: Option<&LymaValue>,
        root: Option<&LymaValue>,
        resolver_context: &mut ResolutionContext,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        let mut entries = Vec::new();
        let mut lexical_scope = scope.fork()?;
        for item in &mapping.items {
            let current = LymaValue::Mapping(LymaMapping {
                entries: entries.clone(),
                duplicate_keys: Vec::new(),
                span: Some(mapping.span),
            });
            let effective_root = root.unwrap_or(&current);
            match item {
                MappingItem::Entry(entry) => {
                    let key = self.evaluate_key(&entry.key, &lexical_scope, state)?;
                    let child = lexical_scope.child(
                        self.engine,
                        Some(path_segment_from_key(&key)),
                        &current,
                        parent,
                        Some(effective_root),
                    )?;
                    let value = self.evaluate_node(
                        &entry.value,
                        &child,
                        Some(&current),
                        Some(effective_root),
                        resolver_context,
                        state,
                    )?;
                    entries.push(LymaMappingEntry {
                        key,
                        value,
                        span: Some(entry.span),
                    });
                }
                MappingItem::Spread(spread) => {
                    let child = lexical_scope.child(
                        self.engine,
                        None,
                        &current,
                        parent,
                        Some(effective_root),
                    )?;
                    let value = self.evaluate_expression(
                        &child,
                        &spread.expression.source,
                        spread.expression.span,
                        state,
                    )?;
                    spread_mapping(&mut entries, value)?;
                }
                MappingItem::Conditional(block) => {
                    if let Some(selected) = self.select_mapping_branch(
                        block,
                        &lexical_scope,
                        &current,
                        Some(effective_root),
                        state,
                    )? {
                        let LymaValue::Mapping(expanded) = self.evaluate_mapping(
                            &selected,
                            &lexical_scope,
                            Some(&current),
                            Some(effective_root),
                            resolver_context,
                            state,
                        )?
                        else {
                            unreachable!()
                        };
                        entries.extend(expanded.entries);
                    }
                }
                MappingItem::Loop(block) => {
                    let iterated = self.evaluate_expression(
                        &lexical_scope,
                        &block.iterable.source,
                        block.iterable.span,
                        state,
                    )?;
                    for loop_item in iter_items(&iterated)? {
                        let child = lexical_scope.child(
                            self.engine,
                            None,
                            &current,
                            parent,
                            Some(effective_root),
                        )?;
                        let mut child = child;
                        bind_loop_values(self.engine, &mut child, &block.bindings, &loop_item)?;
                        let LymaValue::Mapping(expanded) = self.evaluate_mapping(
                            &block.body,
                            &child,
                            Some(&current),
                            Some(effective_root),
                            resolver_context,
                            state,
                        )?
                        else {
                            unreachable!()
                        };
                        entries.extend(expanded.entries);
                    }
                }
                MappingItem::Let(binding) => {
                    let value = self.evaluate_node(
                        &binding.value,
                        &lexical_scope,
                        Some(&current),
                        Some(effective_root),
                        resolver_context,
                        state,
                    )?;
                    lexical_scope.bind_value(self.engine, &binding.name, &value)?;
                }
                MappingItem::Directive(Directive::Include(include)) => {
                    let resolver = self.options.require_resolver("includes")?;
                    let (locator, file, _) = load_lyma_resource(
                        resolver,
                        ResolutionKind::Include,
                        &include.location.value,
                        scope.locator.as_ref(),
                        resolver_context,
                    )?;
                    let value = self
                        .evaluate_document(
                            &file.documents[0],
                            &ResourceContext {
                                locator: Some(locator.clone()),
                                root: scope.root_locator.clone().or(Some(locator)),
                                source_name: include.location.value.clone(),
                            },
                            resolver_context,
                            EvaluationMode::Document,
                        )?
                        .value;
                    spread_mapping(&mut entries, value)?;
                }
                MappingItem::Directive(_) | MappingItem::Comment(_) => {}
            }
        }
        Ok(LymaValue::Mapping(LymaMapping {
            entries,
            duplicate_keys: Vec::new(),
            span: Some(mapping.span),
        }))
    }

    fn evaluate_key(
        &self,
        key: &MappingKey,
        scope: &RuntimeScope<E>,
        state: &DocumentState,
    ) -> Result<LymaKey, EvaluationError> {
        match key {
            MappingKey::Plain { value, .. } => Ok(LymaKey::String(value.clone())),
            MappingKey::Quoted(value) => Ok(LymaKey::String(value.value.clone())),
            MappingKey::Expression { expression, .. } => {
                match self.evaluate_expression(scope, &expression.source, expression.span, state)? {
                    LymaValue::String(value) => Ok(LymaKey::String(value)),
                    LymaValue::Number(value) => Ok(LymaKey::Number(value)),
                    LymaValue::Boolean(value) => Ok(LymaKey::Boolean(value)),
                    _ => Err(EvaluationError::new(
                        lyma_syntax::DiagnosticCode::InvalidExpressionKey,
                        "expression key must evaluate to a string, number, or boolean",
                    )),
                }
            }
        }
    }

    fn evaluate_expression(
        &self,
        scope: &RuntimeScope<E>,
        source: &str,
        span: lyma_syntax::Span,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        Self::reject_unsafe_source(source)?;
        let compiled = self.engine.compile_expression(
            LuaSourceText::new("expr", source).with_span(span),
            &self.options.profile.profile().runtime_limits,
        )?;
        let mut environment = scope.environment.fork_isolated()?;
        let value = self.engine.evaluate_expression(
            &compiled,
            &mut environment,
            &self.options.profile.profile().runtime_limits,
        )?;
        stabilize_runtime_value(
            self.engine,
            &value,
            self.options.profile.profile(),
            Some(span),
            state.data_only,
        )
    }

    fn evaluate_chunk(
        &self,
        scope: &RuntimeScope<E>,
        source: &str,
        span: lyma_syntax::Span,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        Self::reject_unsafe_source(source)?;
        let compiled = self.engine.compile_chunk(
            LuaSourceText::new("chunk", source).with_span(span),
            &self.options.profile.profile().runtime_limits,
        )?;
        let mut environment = scope.environment.fork_isolated()?;
        let value = self.engine.evaluate_chunk(
            &compiled,
            &mut environment,
            &self.options.profile.profile().runtime_limits,
        )?;
        stabilize_runtime_value(
            self.engine,
            &value,
            self.options.profile.profile(),
            Some(span),
            state.data_only,
        )
    }

    fn reject_unsafe_source(source: &str) -> Result<(), EvaluationError> {
        if source_mentions_forbidden_capability(source) {
            return Err(EvaluationError::new(
                lyma_syntax::DiagnosticCode::UnsafeOperation,
                "safe evaluation rejected forbidden runtime capability",
            ));
        }
        Ok(())
    }

    fn merge_top_level(left: LymaValue, right: LymaValue) -> Result<LymaValue, EvaluationError> {
        match (left, right) {
            (LymaValue::Mapping(mut left), LymaValue::Mapping(right)) => {
                left.entries.extend(right.entries);
                Ok(LymaValue::Mapping(left))
            }
            (LymaValue::Sequence(mut left), LymaValue::Sequence(right)) => {
                left.items.extend(right.items);
                Ok(LymaValue::Sequence(left))
            }
            _ => Err(EvaluationError::new(
                lyma_syntax::DiagnosticCode::IncludeTypeMismatch,
                "included top-level value was incompatible with the target root",
            )),
        }
    }

    fn resolve_tag_value(
        &self,
        tagged: &lyma_syntax::TaggedNode,
        value: &LymaValue,
        state: &DocumentState,
    ) -> Result<LymaValue, EvaluationError> {
        let preserved = LymaValue::Tagged(LymaTaggedValue {
            tag: tagged.tag.clone(),
            value: Box::new(value.clone()),
            span: Some(tagged.span),
        });
        match self.options.tag_resolver {
            Some(resolver) => match resolver.resolve_tag(TagResolutionRequest {
                tag: &tagged.tag,
                value,
            }) {
                Ok(value) => stabilize_lyma_value(&value, state.data_only),
                Err(error)
                    if error.diagnostic.code == lyma_syntax::DiagnosticCode::UnknownTag
                        && !state.reject_unknown_tags
                        && matches!(
                            self.options.unknown_tag_policy,
                            UnknownTagPolicy::Preserve
                        ) =>
                {
                    stabilize_lyma_value(&preserved, state.data_only)
                }
                Err(error) => Err(EvaluationError::from(error)),
            },
            None if !state.reject_unknown_tags => stabilize_lyma_value(&preserved, state.data_only),
            None => Err(EvaluationError::new(
                lyma_syntax::DiagnosticCode::UnknownTag,
                format!("unknown tag '!{}'", tagged.tag.name.value),
            )),
        }
    }

    fn plan_document_state(&self, document: &Document) -> Result<DocumentState, EvaluationError> {
        let mut metadata = DocumentMetadata::new();
        for item in &document.items {
            if let DocumentItem::Directive(directive) = item {
                match directive {
                    Directive::Version(value) => metadata.version = Some(value.version.clone()),
                    Directive::Profile(value) => metadata.profile = Some(value.profile.clone()),
                    Directive::Schema(value) => {
                        metadata.schema = Some(value.location.value.clone());
                    }
                    _ => {}
                }
            }
        }

        if matches!(metadata.profile, Some(LymaProfile::Trusted))
            && self.options.profile.profile().name != "trusted"
        {
            return Err(EvaluationError::new(
                lyma_syntax::DiagnosticCode::UnsupportedProfile,
                "document requires trusted evaluation",
            ));
        }

        let data_only =
            metadata.schema.is_some() || matches!(metadata.profile, Some(LymaProfile::Data));
        let reject_unknown_tags = match self.options.unknown_tag_policy {
            UnknownTagPolicy::Reject => true,
            UnknownTagPolicy::Preserve => false,
            UnknownTagPolicy::RejectForSchemaValidatedDocuments => metadata.schema.is_some(),
        };

        Ok(DocumentState {
            metadata,
            data_only,
            reject_unknown_tags,
        })
    }

    fn select_mapping_branch(
        &self,
        block: &lyma_syntax::ConditionalBlock<lyma_syntax::MappingBlock>,
        scope: &RuntimeScope<E>,
        current: &LymaValue,
        root: Option<&LymaValue>,
        state: &DocumentState,
    ) -> Result<Option<lyma_syntax::MappingBlock>, EvaluationError> {
        if is_truthy(&self.evaluate_expression(
            &scope.child(self.engine, None, current, None, root)?,
            &block.if_branch.condition.source,
            block.if_branch.condition.span,
            state,
        )?) {
            return Ok(Some(block.if_branch.body.clone()));
        }
        for branch in &block.else_if_branches {
            if is_truthy(&self.evaluate_expression(
                &scope.child(self.engine, None, current, None, root)?,
                &branch.condition.source,
                branch.condition.span,
                state,
            )?) {
                return Ok(Some(branch.body.clone()));
            }
        }
        Ok(block.else_branch.as_ref().map(|branch| branch.body.clone()))
    }

    fn select_sequence_branch(
        &self,
        block: &lyma_syntax::ConditionalBlock<lyma_syntax::SequenceBlock>,
        scope: &RuntimeScope<E>,
        current: &LymaValue,
        root: Option<&LymaValue>,
        state: &DocumentState,
    ) -> Result<Option<lyma_syntax::SequenceBlock>, EvaluationError> {
        if is_truthy(&self.evaluate_expression(
            &scope.child(self.engine, None, current, None, root)?,
            &block.if_branch.condition.source,
            block.if_branch.condition.span,
            state,
        )?) {
            return Ok(Some(block.if_branch.body.clone()));
        }
        for branch in &block.else_if_branches {
            if is_truthy(&self.evaluate_expression(
                &scope.child(self.engine, None, current, None, root)?,
                &branch.condition.source,
                branch.condition.span,
                state,
            )?) {
                return Ok(Some(branch.body.clone()));
            }
        }
        Ok(block.else_branch.as_ref().map(|branch| branch.body.clone()))
    }
}

fn source_mentions_forbidden_capability(source: &str) -> bool {
    let forbidden_identifiers = [
        "_G",
        "_ENV",
        "io",
        "os",
        "debug",
        "package",
        "require",
        "load",
        "loadfile",
        "dofile",
        "collectgarbage",
        "getmetatable",
        "setmetatable",
        "rawget",
        "rawset",
        "rawlen",
        "rawequal",
        "coroutine",
        "socket",
        "ffi",
        "jit",
    ];
    let forbidden_pairs = [
        ("math", "random"),
        ("math", "randomseed"),
        ("os", "time"),
        ("os", "date"),
        ("string", "dump"),
    ];

    let mut chars = source.chars().peekable();
    let mut last_identifier: Option<String> = None;
    let mut saw_dot = false;
    let mut in_single = false;
    let mut in_double = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' if in_single || in_double => {
                let _ = chars.next();
            }
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ if in_single || in_double => {}
            '.' => saw_dot = last_identifier.is_some(),
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut identifier = String::from(c);
                while let Some(next) = chars.peek() {
                    if next.is_ascii_alphanumeric() || *next == '_' {
                        identifier.push(*next);
                        let _ = chars.next();
                    } else {
                        break;
                    }
                }

                if forbidden_identifiers.contains(&identifier.as_str()) {
                    return true;
                }
                if saw_dot
                    && last_identifier
                        .as_deref()
                        .is_some_and(|left| forbidden_pairs.contains(&(left, identifier.as_str())))
                {
                    return true;
                }

                last_identifier = Some(identifier);
                saw_dot = false;
            }
            c if c.is_whitespace() => {}
            _ => {
                last_identifier = None;
                saw_dot = false;
            }
        }
    }

    false
}

fn bind_loop_values<E: LuaRuntimeEngine>(
    engine: &E,
    scope: &mut RuntimeScope<E>,
    bindings: &LoopBindings,
    loop_item: &crate::control::LoopItem<'_>,
) -> Result<(), EvaluationError> {
    match bindings {
        LoopBindings::One { value, .. } => scope.bind_value(engine, value, loop_item.value),
        LoopBindings::Two { key, value, .. } => {
            if let Some(key_value) = &loop_item.key {
                scope.bind_value(engine, key, key_value)?;
            }
            scope.bind_value(engine, value, loop_item.value)
        }
    }
}

fn parse_number(lexeme: &str) -> Result<LymaValue, EvaluationError> {
    if let Some(hex) = lexeme.strip_prefix("0x") {
        if let Ok(value) = i64::from_str_radix(hex, 16) {
            return Ok(LymaValue::Number(LymaNumber::Integer(value)));
        }
    }
    if let Ok(value) = lexeme.parse::<i64>() {
        return Ok(LymaValue::Number(LymaNumber::Integer(value)));
    }
    lexeme
        .parse::<f64>()
        .map(LymaNumber::Float)
        .map(LymaValue::Number)
        .map_err(|_| {
            EvaluationError::new(
                lyma_syntax::DiagnosticCode::SerializationError,
                "failed to parse numeric literal",
            )
        })
}
