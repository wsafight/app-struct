use appstruct_migrate::IntrospectedSchema;
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

pub(super) fn interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub(super) fn select(
    mut schema: IntrospectedSchema,
) -> io::Result<Option<(IntrospectedSchema, super::render::AccessMode)>> {
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
    let access = prompt_access()?;
    let draft = super::render::render_with_access(&schema, &access);
    println!("\nDraft: {} entities", draft.entity_count);
    for warning in &draft.warnings {
        println!("  Warning: {warning}");
    }
    if matches!(access, super::render::AccessMode::None) {
        println!("Access rules must be declared before adding this draft to includes.");
    }
    if confirm("Write draft? [y/N]: ", false)? {
        Ok(Some((schema, access)))
    } else {
        Ok(None)
    }
}

fn prompt_access() -> io::Result<super::render::AccessMode> {
    println!("\nChoose imported entity access:");
    println!("  1) none          fail closed; add rules manually");
    println!("  2) public        all CRUD operations are public");
    println!("  3) authenticated all CRUD operations require a signed-in user");
    println!("  4) role          all CRUD operations require one RBAC role");
    println!("  5) owner         all CRUD operations require an ownership relation");
    loop {
        let input = prompt_line("Access policy [1]: ", "1")?;
        match input.as_str() {
            "1" | "none" => return Ok(super::render::AccessMode::None),
            "2" | "public" => return Ok(super::render::AccessMode::Public),
            "3" | "authenticated" => return Ok(super::render::AccessMode::Authenticated),
            "4" | "role" => {
                let role = prompt_line("RBAC role: ", "")?;
                if super::valid_access_name(&role) {
                    return Ok(super::render::AccessMode::Role(role));
                }
                eprintln!(
                    "role must start with a lowercase letter and use lowercase letters, digits, or underscores"
                );
            }
            "5" | "owner" => {
                let owner = prompt_line("Ownership relation field: ", "")?;
                if super::valid_access_name(&owner) {
                    return Ok(super::render::AccessMode::Owner(owner));
                }
                eprintln!(
                    "owner field must start with a lowercase letter and use lowercase letters, digits, or underscores"
                );
            }
            _ => eprintln!("access policy must be 1, 2, 3, 4, or 5"),
        }
    }
}

fn prompt_line(prompt: &str, default: &str) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input ended"));
    }
    let input = input.trim();
    Ok(if input.is_empty() {
        default.to_owned()
    } else {
        input.to_owned()
    })
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
