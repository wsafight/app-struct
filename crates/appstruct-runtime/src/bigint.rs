//! Lossless JSON transport for business bigint fields. Rust and PostgreSQL still use i64.

use serde::{Deserialize, Deserializer, Serializer, de};

#[derive(Deserialize)]
#[serde(untagged)]
enum Input {
    Text(String),
    Number(i64),
}

impl Input {
    fn value<E: de::Error>(self) -> Result<i64, E> {
        match self {
            Self::Text(value) => {
                let digits = value.strip_prefix('-').unwrap_or(&value);
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(E::custom("bigint must be a decimal integer string"));
                }
                value.parse().map_err(E::custom)
            }
            Self::Number(value)
                if (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) =>
            {
                Ok(value)
            }
            Self::Number(_) => Err(E::custom(
                "bigint outside the safe JSON integer range must be a string",
            )),
        }
    }
}

/// # Errors
/// Returns the serializer's error if the decimal string cannot be written.
pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

/// # Errors
/// Rejects invalid integers, values outside i64, and unsafe numeric JSON inputs.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    Input::deserialize(deserializer)?.value()
}

pub mod optional {
    use super::{Deserialize, Deserializer, Input, Serializer};

    /// # Errors
    /// Returns the serializer's error if the string or null cannot be written.
    pub fn serialize<S: Serializer>(value: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.collect_str(value),
            None => serializer.serialize_none(),
        }
    }

    /// # Errors
    /// Rejects present values that do not satisfy the bigint transport contract.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<i64>, D::Error> {
        Option::<Input>::deserialize(deserializer)?
            .map(Input::value)
            .transpose()
    }
}

pub mod patch {
    use super::{Deserializer, Serializer, optional};

    /// # Errors
    /// Returns the serializer's error if the patch value cannot be written.
    pub fn serialize<S: Serializer>(
        value: &Option<Option<i64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        optional::serialize(&value.flatten(), serializer)
    }

    /// # Errors
    /// Rejects non-null values that do not satisfy the bigint transport contract.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<i64>>, D::Error> {
        optional::deserialize(deserializer).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    #[allow(clippy::option_option)]
    struct Record {
        #[serde(with = "super")]
        required: i64,
        #[serde(default, with = "super::optional")]
        optional: Option<i64>,
        #[serde(
            default,
            with = "super::patch",
            skip_serializing_if = "Option::is_none"
        )]
        patch: Option<Option<i64>>,
    }

    #[test]
    fn full_i64_range_round_trips_without_numeric_json() {
        for value in [
            i64::MIN,
            -9_007_199_254_740_993,
            0,
            9_007_199_254_740_993,
            i64::MAX,
        ] {
            let record = Record {
                required: value,
                optional: Some(value),
                patch: Some(Some(value)),
            };
            let encoded = serde_json::to_value(&record).unwrap();
            assert_eq!(encoded["required"], value.to_string());
            assert_eq!(serde_json::from_value::<Record>(encoded).unwrap(), record);
        }
    }

    #[test]
    fn patch_preserves_omitted_null_and_present_values() {
        for (input, expected) in [
            (json!({"required": "1"}), None),
            (json!({"required": "1", "patch": null}), Some(None)),
            (json!({"required": "1", "patch": "2"}), Some(Some(2))),
        ] {
            assert_eq!(
                serde_json::from_value::<Record>(input).unwrap().patch,
                expected
            );
        }
    }

    #[test]
    fn accepts_legacy_safe_integers_and_rejects_lossy_or_invalid_inputs() {
        assert_eq!(
            serde_json::from_value::<Record>(json!({"required": 42}))
                .unwrap()
                .required,
            42
        );
        for value in [
            json!(9_007_199_254_740_993_i64),
            json!(1.5),
            json!("1.0"),
            json!("1e3"),
            json!("+1"),
            json!("9223372036854775808"),
        ] {
            assert!(serde_json::from_value::<Record>(json!({"required": value})).is_err());
        }
    }
}
