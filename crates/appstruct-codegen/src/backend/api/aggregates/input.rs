use appstruct_ir::{AggregateIr, EntityIr};
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn support(entity: &EntityIr, aggregate: &AggregateIr) -> TokenStream {
    let relation = &entity
        .fields
        .iter()
        .find(|field| field.id == aggregate.relation)
        .expect("validated relation")
        .rust_name;
    let create_fields = super::super::writable_fields(entity, false)
        .filter(|field| field.id != aggregate.relation)
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>();
    let update_fields = super::super::writable_fields(entity, true)
        .filter(|field| field.id != aggregate.relation)
        .map(|field| field.rust_name.as_str())
        .collect::<Vec<_>>();
    quote! {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct Batch {
            #[serde(default)] pub creates: Vec<CreateRow>,
            #[serde(default)] pub updates: Vec<UpdateRow>,
            #[serde(default)] pub deletes: Vec<DeleteRow>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct CreateRow { pub key: String, pub input: serde_json::Value }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct UpdateRow { pub id: uuid::Uuid, pub revision: i64, pub input: serde_json::Value }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct DeleteRow { pub id: uuid::Uuid, pub revision: i64 }

        pub fn invalid(message: impl Into<String>) -> ApiError {
            ApiError::Validation(vec![FieldViolation { field: "aggregate".into(), message: message.into() }])
        }
        impl Batch {
            pub fn validate(&mut self, limit: usize) -> Result<(), ApiError> {
                let size = self.creates.len() + self.updates.len() + self.deletes.len();
                if size == 0 || size > limit { return Err(invalid("Invalid aggregate operation count")); }
                let mut ids = std::collections::BTreeSet::new();
                for (id, revision) in self.updates.iter().map(|row| (row.id, row.revision))
                    .chain(self.deletes.iter().map(|row| (row.id, row.revision))) {
                    if revision < 1 || !ids.insert(id) { return Err(invalid("Duplicate row ID or invalid revision")); }
                }
                let mut keys = std::collections::BTreeSet::new();
                for row in &self.creates {
                    if row.key.is_empty() || row.key.len() > 128 || !keys.insert(&row.key) {
                        return Err(invalid("Create keys must be unique and contain 1 to 128 bytes"));
                    }
                }
                self.deletes.sort_by_key(|row| row.id);
                self.updates.sort_by_key(|row| row.id);
                Ok(())
            }
        }
        fn decode<T: serde::de::DeserializeOwned>(mut value: serde_json::Value, parent: uuid::Uuid, create: bool) -> Result<T, ApiError> {
            let object = value.as_object_mut().ok_or_else(|| invalid("Row input must be an object"))?;
            let allowed: &[&str] = if create { &[#(#create_fields),*] } else { &[#(#update_fields),*] };
            if object.keys().any(|key| !allowed.contains(&key.as_str())) {
                return Err(invalid("Row input contains an unknown or read-only field"));
            }
            if create { object.insert(#relation.to_owned(), serde_json::json!(parent)); }
            serde_json::from_value(value).map_err(|error| invalid(error.to_string()))
        }
        pub fn row_error(error: ApiError, path: &str) -> ApiError {
            match error {
                ApiError::Validation(fields) => ApiError::Validation(fields.into_iter().map(|mut field| {
                    field.field = format!("{path}.{}", field.field); field
                }).collect()),
                error => error,
            }
        }
    }
}
