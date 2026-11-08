// Subscription metadata is not authoritative for entitlements: the current Stripe items are.
fn plan_id_from_subscription(
    config: &BillingConfig,
    subscription: &Value,
) -> Result<&'static str, ApiError> {
    let items = subscription
        .pointer("/items/data")
        .and_then(Value::as_array)
        .ok_or(ApiError::Internal)?;
    if items.len() != 1
        || subscription
            .pointer("/items/has_more")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(ApiError::Internal);
    }
    let price_id = items[0]
        .pointer("/price/id")
        .and_then(Value::as_str)
        .ok_or(ApiError::Internal)?;
    let mut matches = config
        .prices
        .iter()
        .filter(|(_, configured_price)| configured_price.as_str() == price_id)
        .map(|(plan_id, _)| *plan_id);
    let plan_id = matches.next().ok_or(ApiError::Internal)?;
    if matches.next().is_some() {
        return Err(ApiError::Internal);
    }
    Ok(plan_id)
}
