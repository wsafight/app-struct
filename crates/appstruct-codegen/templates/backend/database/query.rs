use super::scalar_value;
use crate::ApiError;
use appstruct_runtime::FilterExpression;
use sea_query::{Alias, Condition, Expr, ExprTrait as _, SimpleExpr};
use sift_query::{CursorValue, NullOrder, Operator, OrderBy, SearchEntity, SortOrder};
use std::sync::OnceLock;

pub const MAX_CURSOR_BYTES: usize = 16_384;

fn invalid(message: impl Into<String>) -> ApiError { ApiError::InvalidQuery(message.into()) }

pub fn query_policy() -> &'static sift_query::QueryPolicy {
    static POLICY: OnceLock<sift_query::QueryPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        use sift_query::postgres::{PgCapabilities, JsonQuery, jsonb};
        [JsonQuery::Contains, JsonQuery::ContainedBy, JsonQuery::Key].into_iter()
            .fold(PgCapabilities::default(), |capabilities, mode| capabilities.with_operator(jsonb(mode)))
            .apply(sift_query::QueryPolicy::default()).expect("valid generated query policy")
    })
}

fn column<E: SearchEntity>(id: sift_query::FieldId) -> Result<Expr, ApiError> {
    let field = E::META.field(id).ok_or(ApiError::Internal)?;
    Ok(Expr::col((Alias::new(E::META.from), Alias::new(field.column))))
}

/// Applies the same sift type/operator validation to every branch of a bounded filter tree.
/// The application checks field read access before any condition is translated.
pub fn filter_expression<E: SearchEntity>(
    expression: &FilterExpression, allowed: &impl Fn(&str) -> bool,
) -> Result<Condition, ApiError> {
    match expression {
        FilterExpression::All { all } => all.iter().try_fold(Condition::all(), |condition, child| {
            Ok(condition.add(filter_expression::<E>(child, allowed)?))
        }),
        FilterExpression::Any { any } => any.iter().try_fold(Condition::any(), |condition, child| {
            Ok(condition.add(filter_expression::<E>(child, allowed)?))
        }),
        FilterExpression::Condition { field, op, values } => {
            let meta = E::META.resolve_field(field).filter(|meta| meta.filterable)
                .ok_or_else(|| invalid(format!("field `{field}` is not filterable")))?;
            if !allowed(field) { return Err(ApiError::Forbidden); }
            let mut entries = vec![(format!("{field}-op"), op.sift_name().to_owned())];
            entries.extend(values.iter().enumerate().map(|(index, value)| (format!("{field}-{index}"), value.clone())));
            let policy = query_policy();
            let raw = sift_query::RawParams::from_entries(entries, policy.limits()).map_err(|e| invalid(e.to_string()))?;
            let query = sift_query::ParamParser::new(policy).and_then(|parser| parser.parse::<E>(&raw))
                .map_err(|e| invalid(e.to_string()))?;
            let arena = query.predicate();
            let Some(sift_query::PredicateNode::Condition(predicate)) = arena.root().and_then(|root| arena.node(root)) else {
                return Err(ApiError::Internal);
            };
            let expression = column::<E>(meta.id)?;
            let values = predicate.values.iter().cloned().map(scalar_value).collect::<Result<Vec<_>, _>>()?;
            let value = || values.first().cloned().ok_or(ApiError::Internal);
            let result = match predicate.operator {
                Operator::Eq => expression.eq(value()?), Operator::Ne => expression.ne(value()?),
                Operator::Gt => expression.gt(value()?), Operator::Ge => expression.gte(value()?),
                Operator::Lt => expression.lt(value()?), Operator::Le => expression.lte(value()?),
                Operator::InList => expression.is_in(values), Operator::NotIn => expression.is_not_in(values),
                Operator::IsNull => expression.is_null(), Operator::NotNull => expression.is_not_null(),
                Operator::Contain | Operator::StartWith | Operator::EndWith => {
                    let sift_query::ScalarValue::Text(text) = &predicate.values[0] else { return Err(ApiError::Internal); };
                    let mut pattern = appstruct_runtime::like_contains_pattern(text);
                    if predicate.operator == Operator::StartWith { pattern.remove(0); }
                    if predicate.operator == Operator::EndWith { pattern.pop(); }
                    expression.like(sea_query::LikeExpr::new(pattern).escape('\\'))
                }
                Operator::Custom(operator) => {
                    let template = match operator.name() {
                        "json_contains" => "$1 @> $2", "json_contained_by" => "$1 <@ $2", "json_key" => "$1 ? $2",
                        _ => return Err(ApiError::Internal),
                    };
                    Expr::cust_with_exprs(template, [expression.into(), Expr::val(value()?).into()])
                }
                _ => return Err(invalid("unsupported filter operator")),
            };
            Ok(Condition::all().add(result))
        }
    }
}

pub fn resource_order<E: SearchEntity>(sort: Option<&str>, allowed: impl Fn(&str) -> bool) -> Result<Vec<OrderBy>, ApiError> {
    let mut order = Vec::new();
    if let Some(sort) = sort {
        if sort.is_empty() || sort.len() > 512 { return Err(invalid("invalid sort")); }
        for item in sort.split(',') {
            let (direction, name) = item.strip_prefix('-').map_or((SortOrder::Asc, item), |name| (SortOrder::Desc, name));
            let meta = E::META.resolve_field(name).filter(|field| field.sortable)
                .ok_or_else(|| invalid(format!("field `{name}` is not sortable")))?;
            if !allowed(name) { return Err(ApiError::Forbidden); }
            if order.iter().any(|item: &OrderBy| item.field == meta.id) { return Err(invalid("duplicate sort field")); }
            order.push(OrderBy { field: meta.id, direction, nulls: direction.into() });
        }
    }
    for stable in E::META.stable_order {
        let field = E::META.field(stable.field).ok_or(ApiError::Internal)?;
        if !allowed(field.public_name) { return Err(ApiError::Forbidden); }
        if !order.iter().any(|item| item.field == stable.field) { order.push((*stable).into()); }
    }
    if order.len() > 4 { return Err(invalid("sort supports at most four fields including the primary key")); }
    Ok(order)
}

/// Lexicographic keyset predicate with PostgreSQL NULL ordering, including reverse traversal.
pub fn cursor_condition<E: SearchEntity>(order: &[OrderBy], values: &[CursorValue], previous: bool) -> Result<Condition, ApiError> {
    if order.len() != values.len() { return Err(invalid("cursor value count changed")); }
    let mut result = Condition::any();
    let mut prefix = Condition::all();
    for (item, value) in order.iter().zip(values) {
        let expression = column::<E>(item.field)?;
        let nulls_first = (item.nulls == NullOrder::First) ^ previous;
        let ascending = (item.direction == SortOrder::Asc) ^ previous;
        let next: Condition = match value {
            CursorValue::Null if nulls_first => Condition::all().add(expression.clone().is_not_null()),
            CursorValue::Null => Condition::all().add(Expr::cust("FALSE")),
            CursorValue::Scalar(value) => {
                let value = scalar_value(value.clone())?;
                let comparison: SimpleExpr = if ascending { expression.clone().gt(value) } else { expression.clone().lt(value) };
                if nulls_first { Condition::all().add(comparison) }
                else { Condition::any().add(comparison).add(expression.clone().is_null()) }
            }
        };
        result = result.add(prefix.clone().add(next));
        prefix = prefix.add(match value {
            CursorValue::Null => expression.is_null(),
            CursorValue::Scalar(value) => expression.eq(scalar_value(value.clone())?),
        });
    }
    Ok(result)
}
