use appstruct_migrate::IntrospectedSchema;
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

pub(super) fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub(super) fn select(mut schema: IntrospectedSchema) -> io::Result<Option<IntrospectedSchema>> {
    println!(
        "PostgreSQL schema `{}`: {} tables",
        schema.name,
        schema.tables.len()
    );
    let mut selected = BTreeSet::new();
    for table in &schema.tables {
        println!("\n{} ({} columns)", table.name, table.columns.len());
        if table.primary_key.len() != 1 {
            println!("  Warning: AppStruct requires one primary-key column");
        }
        for column in &table.columns {
            let flags = if table.primary_key.contains(&column.name) {
                " [primary key]"
            } else if !column.nullable {
                " [required]"
            } else {
                ""
            };
            println!("  {}: {}{}", column.name, column.data_type, flags);
        }
        for key in schema
            .foreign_keys
            .iter()
            .filter(|key| key.source_table == table.name)
        {
            println!(
                "  Relation: {} -> {}.{}",
                key.source_columns.join(", "),
                key.target_table,
                key.target_columns.join(", ")
            );
        }
        if confirm("Import this table? [Y/n]: ", true)? {
            selected.insert(table.name.clone());
        }
    }
    if selected.is_empty() {
        println!("No tables selected");
        return Ok(None);
    }
    schema.tables.retain(|table| selected.contains(&table.name));
    schema
        .foreign_keys
        .retain(|key| selected.contains(&key.source_table) && selected.contains(&key.target_table));
    let draft = super::render::render(&schema);
    println!("\nDraft: {} entities", draft.entity_count);
    for warning in &draft.warnings {
        println!("  Warning: {warning}");
    }
    println!("Access rules must be declared before adding this draft to includes.");
    if confirm("Write draft? [y/N]: ", false)? {
        Ok(Some(schema))
    } else {
        Ok(None)
    }
}

fn confirm(prompt: &str, default: bool) -> io::Result<bool> {
    loop {
        print!("{prompt}");
        io::stdout().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input ended"));
        }
        match input.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => eprintln!("Enter y or n"),
        }
    }
}
