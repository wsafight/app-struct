use appstruct_migrate::IntrospectedColumn;

#[derive(Clone)]
pub(super) enum SpecType {
    Uuid,
    String,
    Text,
    Integer,
    Bigint,
    Decimal,
    Boolean,
    Date,
    Datetime,
    Json,
    Enum(Vec<String>),
    Unsupported(String),
}

impl SpecType {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Uuid => "uuid",
            Self::String => "string",
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Bigint => "bigint",
            Self::Decimal => "decimal",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::Datetime => "datetime",
            Self::Json | Self::Unsupported(_) => "json",
            Self::Enum(_) => "enum",
        }
    }

    pub(super) fn supports_primary_key(&self) -> bool {
        matches!(self, Self::Uuid | Self::Integer | Self::Bigint)
    }
}

pub(super) fn spec_type(column: &IntrospectedColumn) -> SpecType {
    if !column.enum_values.is_empty() {
        return SpecType::Enum(column.enum_values.clone());
    }
    match column.data_type.as_str() {
        "uuid" => SpecType::Uuid,
        "character" | "character varying" => SpecType::String,
        "text" => SpecType::Text,
        "smallint" | "integer" => SpecType::Integer,
        "bigint" => SpecType::Bigint,
        "numeric" | "decimal" | "real" | "double precision" => SpecType::Decimal,
        "boolean" => SpecType::Boolean,
        "date" => SpecType::Date,
        "timestamp with time zone" | "timestamp without time zone" => SpecType::Datetime,
        "json" | "jsonb" => SpecType::Json,
        other => SpecType::Unsupported(other.to_owned()),
    }
}
