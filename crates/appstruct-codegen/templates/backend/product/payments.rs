use super::*;
use crate::database::Database;

pub async fn apply_billing_event(database: &Database, event: &Value) -> Result<(), ApiError> {
    if !__BILLING__ { return Ok(()); }
    let kind = event["type"].as_str().unwrap_or("");
    let object = &event["data"]["object"];
    let checkout = matches!(kind, "checkout.session.completed" | "checkout.session.async_payment_succeeded") && object["mode"] == "payment" && object["metadata"]["appstruct_credit_pack"] == "v1";
    let charge = matches!(kind, "charge.succeeded" | "charge.captured" | "charge.refunded");
    if !checkout && !charge { return Ok(()); }
    let event_id = event["id"].as_str().ok_or(ApiError::Internal)?;
    let transaction = database.begin().await?;
    transaction.fetch_optional(Sql::bind("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", [format!("product-payment:{event_id}").into()])).await?;
    if transaction.fetch_optional(Sql::bind("SELECT 1 FROM \"_appstruct_billing_events\" WHERE event_id = $1", [event_id.into()])).await?.is_some() { return Ok(()); }
    if checkout && object["payment_status"] == "paid" {
        let session = object["id"].as_str().ok_or(ApiError::Internal)?;
        let row = transaction.fetch_optional(Sql::bind("SELECT tenant_id FROM \"_appstruct_product_credit_purchases\" WHERE session_id = $1", [session.into()])).await?.ok_or(ApiError::Internal)?;
        let tenant = row.try_get::<uuid::Uuid, _>("tenant_id")?;
        transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
        let row = transaction.fetch_optional(Sql::bind("UPDATE \"_appstruct_product_credit_purchases\" SET fulfilled = true, payment_intent = $2 WHERE session_id = $1 AND NOT fulfilled RETURNING credits", [session.into(), object["payment_intent"].as_str().into()])).await?;
        if let Some(row) = row { credits::ledger(&transaction, tenant, None, &format!("purchase:{session}"), row.try_get("credits")?, "purchase", None).await?; }
        refund_purchase(&transaction, session).await?;
    }
    if charge && object["paid"].as_bool() == Some(true) {
        let customer = object["customer"].as_str().ok_or(ApiError::Internal)?;
        let row = transaction.fetch_optional(Sql::bind("SELECT organization_id FROM \"_appstruct_billing_customers\" WHERE provider_customer_id = $1", [customer.into()])).await?;
        if let Some(row) = row {
            let tenant = row.try_get::<uuid::Uuid, _>("organization_id")?;
            transaction.fetch_optional(Sql::bind("SELECT id FROM \"_appstruct_tenant_organizations\" WHERE id = $1 FOR NO KEY UPDATE", [tenant.into()])).await?.ok_or(ApiError::InvalidTenant)?;
            let charge_id = object["id"].as_str().ok_or(ApiError::Internal)?;
            let currency = object["currency"].as_str().filter(|value| value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_lowercase())).ok_or(ApiError::Internal)?;
            let captured = object["amount_captured"].as_i64().unwrap_or(0).max(0);
            let refunded = object["amount_refunded"].as_i64().unwrap_or(0).max(0);
            let paid_at = object["created"].as_i64().and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0)).ok_or(ApiError::Internal)?;
            let intent = object["payment_intent"].as_str();
            transaction.execute(Sql::bind("INSERT INTO \"_appstruct_product_payments\" (charge_id, tenant_id, payment_intent, currency, captured, refunded, paid_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP) ON CONFLICT (charge_id) DO UPDATE SET captured = GREATEST(\"_appstruct_product_payments\".captured, EXCLUDED.captured), refunded = GREATEST(\"_appstruct_product_payments\".refunded, EXCLUDED.refunded), updated_at = CURRENT_TIMESTAMP", [charge_id.into(), tenant.into(), intent.into(), currency.into(), captured.into(), refunded.into(), paid_at.into()])).await?;
            if let Some(intent) = intent {
                let purchases = transaction.fetch_all(Sql::bind("SELECT session_id FROM \"_appstruct_product_credit_purchases\" WHERE payment_intent = $1 AND fulfilled", [intent.into()])).await?;
                for row in purchases { refund_purchase(&transaction, &row.try_get::<String, _>("session_id")?).await?; }
            }
        }
    }
    transaction.execute(Sql::bind("INSERT INTO \"_appstruct_billing_events\" (event_id, event_type, payload, created_at) VALUES ($1, $2, $3, CURRENT_TIMESTAMP) ON CONFLICT (event_id) DO NOTHING", [event_id.into(), kind.into(), event.clone().into()])).await?;
    transaction.commit().await?;
    Ok(())
}

async fn refund_purchase<C: DatabaseExecutor>(database: &C, session: &str) -> Result<(), ApiError> {
    let row = database.fetch_optional(Sql::bind("SELECT p.tenant_id, p.credits, p.refunded_credits, COALESCE(SUM(c.captured), 0)::bigint AS captured, COALESCE(SUM(c.refunded), 0)::bigint AS refunded FROM \"_appstruct_product_credit_purchases\" p LEFT JOIN \"_appstruct_product_payments\" c ON c.payment_intent = p.payment_intent WHERE p.session_id = $1 AND p.fulfilled GROUP BY p.tenant_id, p.credits, p.refunded_credits", [session.into()])).await?;
    if let Some(row) = row {
        let captured: i64 = row.try_get("captured")?;
        if captured == 0 { return Ok(()); }
        let refunded: i64 = row.try_get("refunded")?;
        let units: i64 = row.try_get("credits")?;
        let previous: i64 = row.try_get("refunded_credits")?;
        let total = i64::try_from(i128::from(units) * i128::from(refunded.min(captured)) / i128::from(captured)).map_err(|_| ApiError::Internal)?;
        if total > previous {
            let tenant: uuid::Uuid = row.try_get("tenant_id")?;
            credits::ledger(database, tenant, None, &format!("payment-refund:{session}:{total}"), previous - total, "payment_refund", None).await?;
            database.execute(Sql::bind("UPDATE \"_appstruct_product_credit_purchases\" SET refunded_credits = $2 WHERE session_id = $1", [session.into(), total.into()])).await?;
        }
    }
    Ok(())
}
