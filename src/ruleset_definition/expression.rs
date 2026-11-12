//! Refs and expressions of a Ruleset Definition (ADR-0015, ADR-0016).
//!
//! A ref names a param, and a param may take several card types, so a ref is checked against every
//! card type its param may take: its **possible card types**. Checking is depth-first, left to
//! right: all the operands of an expression are checked (each with its own errors) before the type
//! rule of their operator.

use super::draft::{ExpressionDraft, RefDraft};
use super::error::{DefinitionError, DefinitionErrorKind, RefType, ReferenceTarget};
use super::model::{
    CardRef, CardType, CardTypeId, CardValue, Expression, FieldType, ParamAlternative, ParamName,
    ValueRef, Zone,
};
use crate::definition_path::DefinitionPath;
use crate::id::is_valid_id;

pub(super) fn fail<T>(
    path: DefinitionPath,
    kind: DefinitionErrorKind,
) -> Result<T, DefinitionError> {
    Err(DefinitionError { path, kind })
}

/// A name a ref may start with, and the card types the card behind it may have.
#[derive(Debug, Clone)]
pub(super) struct ScopedParam {
    pub(super) name: ParamName,
    /// The possible card types, in declaration order, without repeats. Never empty.
    pub(super) types: Vec<CardTypeId>,
}

impl ScopedParam {
    /// A param that takes the card types of its alternatives, in order, each once.
    pub(super) fn of_alternatives(name: ParamName, alternatives: &[ParamAlternative]) -> Self {
        let mut types: Vec<CardTypeId> = Vec::with_capacity(alternatives.len());
        for alternative in alternatives {
            if !types.contains(&alternative.card_type) {
                types.push(alternative.card_type.clone());
            }
        }
        Self { name, types }
    }
}

/// What the refs of an action (or of a win condition) are checked against.
#[derive(Debug, Clone)]
pub(super) struct Refs<'a> {
    pub(super) zones: &'a [Zone],
    pub(super) card_types: &'a [CardType],
    pub(super) params: Vec<ScopedParam>,
    /// What a ref to a name nothing declares is a dangling reference to: a param, or for a win
    /// condition a card type.
    pub(super) missing: ReferenceTarget,
}

impl Refs<'_> {
    /// The possible card types of `param`; empty if it is not in scope.
    pub(super) fn types_of(&self, param: &ParamName) -> &[CardTypeId] {
        self.params
            .iter()
            .find(|candidate| &candidate.name == param)
            .map_or(&[], |scoped| scoped.types.as_slice())
    }

    /// The same scope where `param` may take only `types`.
    pub(super) fn narrowed(&self, param: &ParamName, types: Vec<CardTypeId>) -> Self {
        let mut narrowed = self.clone();
        if let Some(scoped) = narrowed
            .params
            .iter_mut()
            .find(|candidate| &candidate.name == param)
        {
            scoped.types = types;
        }
        narrowed
    }

    fn card_type(&self, id: &CardTypeId) -> Option<&CardType> {
        self.card_types.iter().find(|card_type| &card_type.id == id)
    }
}

/// A ref taken apart: the param it names and, for `<param>.<name>`, the name.
struct ParsedRef<'a> {
    param: &'a str,
    name: Option<&'a str>,
}

/// `<id>` or `<id>.<id>`.
fn parse_ref<'a>(
    reference: &'a RefDraft,
    at: &DefinitionPath,
) -> Result<ParsedRef<'a>, DefinitionError> {
    let invalid = || DefinitionError {
        path: at.clone(),
        kind: DefinitionErrorKind::InvalidReference {
            reference: reference.0.clone(),
        },
    };
    let (param, name) = match reference.0.split_once('.') {
        Some((param, name)) => (param, Some(name)),
        None => (reference.0.as_str(), None),
    };
    if !is_valid_id(param) || name.is_some_and(|name| !is_valid_id(name)) {
        return Err(invalid());
    }
    Ok(ParsedRef { param, name })
}

/// The param a ref names, or the dangling reference.
fn find_param<'r>(
    refs: &'r Refs<'_>,
    param: &str,
    at: &DefinitionPath,
) -> Result<&'r ScopedParam, DefinitionError> {
    refs.params
        .iter()
        .find(|declared| declared.name.as_str() == param)
        .ok_or_else(|| DefinitionError {
            path: at.clone(),
            kind: DefinitionErrorKind::DanglingReference {
                target: refs.missing,
                id: param.to_string(),
            },
        })
}

/// A ref that must be a card: `{ ref: <param> }`.
pub(super) fn validate_card_ref(
    reference: &RefDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<CardRef, DefinitionError> {
    let parsed = parse_ref(reference, &at)?;
    let param = find_param(refs, parsed.param, &at)?;
    match parsed.name {
        None => Ok(CardRef {
            param: param.name.clone(),
        }),
        Some(name) => {
            let (_, found) = resolve_name(refs, param, name, &at)?;
            fail(
                at,
                DefinitionErrorKind::ReferenceTypeMismatch {
                    expected: RefType::Card,
                    found: RefType::Value(found),
                },
            )
        }
    }
}

/// A ref that must be a value of type `expected`: `{ ref: <param>.<name> }`.
pub(super) fn validate_value_ref(
    reference: &RefDraft,
    expected: FieldType,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<ValueRef, DefinitionError> {
    let parsed = parse_ref(reference, &at)?;
    let param = find_param(refs, parsed.param, &at)?;
    let Some(name) = parsed.name else {
        return fail(
            at,
            DefinitionErrorKind::ReferenceTypeMismatch {
                expected: RefType::Value(expected),
                found: RefType::Card,
            },
        );
    };
    let (value, found) = resolve_name(refs, param, name, &at)?;
    if found != expected {
        return fail(
            at,
            DefinitionErrorKind::ReferenceTypeMismatch {
                expected: RefType::Value(expected),
                found: RefType::Value(found),
            },
        );
    }
    Ok(ValueRef {
        param: param.name.clone(),
        value,
    })
}

/// What `name` is on every possible card type of `param`: a field or an instance state, and its
/// type. The card types are checked in order: a type that declares the name as both is ambiguous,
/// a type that lacks it is dangling, and a kind or type that differs from the first card type's is
/// inconsistent.
fn resolve_name(
    refs: &Refs<'_>,
    param: &ScopedParam,
    name: &str,
    at: &DefinitionPath,
) -> Result<(CardValue, FieldType), DefinitionError> {
    let mut first: Option<(&CardTypeId, CardValue, FieldType)> = None;
    for id in &param.types {
        let card_type = refs.card_type(id);
        let field = card_type.and_then(|card_type| {
            card_type
                .fields
                .iter()
                .find(|field| field.name.as_str() == name)
        });
        let state = card_type.and_then(|card_type| {
            card_type
                .instance_state
                .iter()
                .find(|state| state.name.as_str() == name)
        });
        let (value, found) = match (field, state) {
            (Some(_), Some(_)) => {
                return fail(
                    at.clone(),
                    DefinitionErrorKind::AmbiguousReference {
                        name: name.to_string(),
                    },
                )
            }
            (Some(field), None) => (CardValue::Field(field.name.clone()), field.field_type),
            (None, Some(state)) => (CardValue::State(state.name.clone()), state.state_type),
            (None, None) => {
                return fail(
                    at.clone(),
                    DefinitionErrorKind::DanglingReference {
                        target: ReferenceTarget::FieldOrInstanceState,
                        id: name.to_string(),
                    },
                )
            }
        };
        match &first {
            None => first = Some((id, value, found)),
            Some((first_id, first_value, first_found)) => {
                if *first_value != value || *first_found != found {
                    return fail(
                        at.clone(),
                        DefinitionErrorKind::InconsistentReference {
                            name: name.to_string(),
                            card_type: first_id.as_str().to_string(),
                            other: id.as_str().to_string(),
                        },
                    );
                }
            }
        }
    }
    match first {
        Some((_, value, found)) => Ok((value, found)),
        None => fail(
            at.clone(),
            DefinitionErrorKind::DanglingReference {
                target: refs.missing,
                id: param.name.as_str().to_string(),
            },
        ),
    }
}

/// An expression that must be bool: a `where`, an `if` or a `when`.
pub(super) fn check_condition(
    draft: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<Expression, DefinitionError> {
    let (expression, found) = check_expression(draft, refs, at.clone())?;
    if found != FieldType::Bool {
        return fail(
            at,
            DefinitionErrorKind::ExpressionTypeMismatch {
                expected: FieldType::Bool,
                found,
            },
        );
    }
    Ok(expression)
}

/// An expression and its type. `at` is its path; the operands of an operator are at the operator's
/// key and their position.
fn check_expression(
    draft: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<(Expression, FieldType), DefinitionError> {
    match draft {
        ExpressionDraft::Literal(value) => {
            Ok((Expression::Literal(value.clone()), value.field_type()))
        }
        ExpressionDraft::Ref(reference) => {
            let parsed = parse_ref(reference, &at)?;
            let param = find_param(refs, parsed.param, &at)?;
            let Some(name) = parsed.name else {
                return fail(
                    at,
                    DefinitionErrorKind::CardRefNotAValue {
                        reference: reference.0.clone(),
                    },
                );
            };
            let (value, found) = resolve_name(refs, param, name, &at)?;
            Ok((
                Expression::Value(ValueRef {
                    param: param.name.clone(),
                    value,
                }),
                found,
            ))
        }
        ExpressionDraft::Eq(left, right) => {
            let at = at.key("eq");
            let (left, expected) = check_expression(left, refs, at.clone().index(0))?;
            let (right, found) = check_expression(right, refs, at.clone().index(1))?;
            if expected != found {
                return fail(
                    at.index(1),
                    DefinitionErrorKind::ExpressionTypeMismatch { expected, found },
                );
            }
            Ok((
                Expression::Eq(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Gte(left, right) => {
            let (left, right) = check_ints("gte", left, right, refs, at)?;
            Ok((
                Expression::Gte(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::Lte(left, right) => {
            let (left, right) = check_ints("lte", left, right, refs, at)?;
            Ok((
                Expression::Lte(Box::new(left), Box::new(right)),
                FieldType::Bool,
            ))
        }
        ExpressionDraft::And(operands) => {
            let at = at.key("and");
            let mut checked = Vec::with_capacity(operands.len());
            let mut types = Vec::with_capacity(operands.len());
            for (index, operand) in operands.iter().enumerate() {
                let (operand, found) = check_expression(operand, refs, at.clone().index(index))?;
                checked.push(operand);
                types.push(found);
            }
            if let Some((index, &found)) = types
                .iter()
                .enumerate()
                .find(|(_, found)| **found != FieldType::Bool)
            {
                return fail(
                    at.index(index),
                    DefinitionErrorKind::ExpressionTypeMismatch {
                        expected: FieldType::Bool,
                        found,
                    },
                );
            }
            Ok((Expression::And(checked), FieldType::Bool))
        }
        ExpressionDraft::IsCardType { card, card_type } => {
            let at = at.key("is_card_type");
            let card = validate_card_ref(card, refs, at.clone().index(0))?;
            if refs.card_type(&CardTypeId::new(card_type)).is_none() {
                return fail(
                    at.index(1),
                    DefinitionErrorKind::DanglingReference {
                        target: ReferenceTarget::CardType,
                        id: card_type.clone(),
                    },
                );
            }
            Ok((
                Expression::IsCardType {
                    card,
                    card_type: CardTypeId::new(card_type),
                },
                FieldType::Bool,
            ))
        }
    }
}

/// The two operands of `gte` or `lte`: both int, the first that is not reported.
fn check_ints(
    operator: &'static str,
    left: &ExpressionDraft,
    right: &ExpressionDraft,
    refs: &Refs<'_>,
    at: DefinitionPath,
) -> Result<(Expression, Expression), DefinitionError> {
    let at = at.key(operator);
    let (left, left_type) = check_expression(left, refs, at.clone().index(0))?;
    let (right, right_type) = check_expression(right, refs, at.clone().index(1))?;
    for (index, found) in [left_type, right_type].into_iter().enumerate() {
        if found != FieldType::Int {
            return fail(
                at.index(index),
                DefinitionErrorKind::ExpressionTypeMismatch {
                    expected: FieldType::Int,
                    found,
                },
            );
        }
    }
    Ok((left, right))
}

/// The params an expression reads, each once, in the order a depth-first, left-to-right walk first
/// meets them.
pub(super) fn params_read(expression: &Expression) -> Vec<&ParamName> {
    fn walk<'a>(expression: &'a Expression, found: &mut Vec<&'a ParamName>) {
        fn note<'a>(found: &mut Vec<&'a ParamName>, param: &'a ParamName) {
            if !found.contains(&param) {
                found.push(param);
            }
        }
        match expression {
            Expression::Literal(_) => {}
            Expression::Value(value) => note(found, &value.param),
            Expression::IsCardType { card, .. } => note(found, &card.param),
            Expression::Eq(left, right)
            | Expression::Gte(left, right)
            | Expression::Lte(left, right) => {
                walk(left, found);
                walk(right, found);
            }
            Expression::And(operands) => {
                for operand in operands {
                    walk(operand, found);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(expression, &mut found);
    found
}
