use appstruct_generated_backend::{FileError, FileState, MailState, RequestContext};
use sea_orm::{ConnectionTrait, Database};
use std::{env, fs, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = Database::connect(env::var("DATABASE_URL")?).await?;
    let tenant_id = env::var("TENANT_ID")?.parse::<uuid::Uuid>()?;
    let root = PathBuf::from(env::var("APPSTRUCT_FILE_ROOT")?);
    let mail = MailState::from_env(database.clone())?;
    let file = FileState::from_env(database.clone())?;
    database
        .execute_unprepared("DELETE FROM \"_appstruct_files\"")
        .await?;
    let context =
        RequestContext::connection_with_file(&database, &mail, &file, None, Some(tenant_id));

    let metadata = context
        .put_file(
            "documents/hello.txt",
            "hello.txt",
            "text/plain",
            b"hello AppStruct",
        )
        .await?;
    assert_eq!(metadata.tenant_id, Some(tenant_id));
    assert_eq!(metadata.size, 15);
    assert_eq!(metadata.checksum.len(), 64);
    assert_eq!(fs::read(root.join("documents/hello.txt"))?, b"hello AppStruct");

    let (loaded, content) = context.get_file("documents/hello.txt").await?;
    assert_eq!(loaded.id, metadata.id);
    assert_eq!(content, b"hello AppStruct");
    assert!(matches!(
        context
            .put_file("documents/hello.txt", "other.txt", "text/plain", b"replace")
            .await,
        Err(FileError::Storage(_))
    ));
    assert_eq!(context.get_file("documents/hello.txt").await?.1, b"hello AppStruct");

    let other = RequestContext::connection_with_file(
        &database,
        &mail,
        &file,
        None,
        Some(uuid::Uuid::now_v7()),
    );
    assert!(matches!(
        other.get_file("documents/hello.txt").await,
        Err(FileError::Database(_))
    ));

    for key in ["../secret", "/tmp/secret", "documents//bad", "documents/./bad", "documents/"] {
        assert!(matches!(
            context.put_file(key, "bad.txt", "text/plain", b"bad").await,
            Err(FileError::InvalidKey)
        ));
    }
    assert!(matches!(
        context
            .put_file("documents/bad-name", "../bad.txt", "text/plain", b"bad")
            .await,
        Err(FileError::InvalidName)
    ));
    assert!(matches!(
        context
            .put_file("documents/fake.png", "fake.png", "image/png", b"not a png")
            .await,
        Err(FileError::InvalidContentType)
    ));
    assert!(matches!(
        context
            .put_file("documents/bad.txt", "bad.txt", "text/plain", &[0xff])
            .await,
        Err(FileError::InvalidContentType)
    ));
    assert!(matches!(
        context
            .put_file("documents/bad.json", "bad.json", "application/json", b"{")
            .await,
        Err(FileError::InvalidContentType)
    ));
    assert!(matches!(
        context
            .put_file("documents/large.txt", "large.txt", "text/plain", &vec![b'x'; 1025])
            .await,
        Err(FileError::TooLarge { .. })
    ));

    fs::write(root.join("documents/hello.txt"), b"tampered")?;
    assert!(matches!(
        context.get_file("documents/hello.txt").await,
        Err(FileError::Storage(message)) if message.contains("checksum")
    ));
    context.delete_file("documents/hello.txt").await?;
    assert!(!root.join("documents/hello.txt").exists());
    assert!(matches!(
        context.get_file("documents/hello.txt").await,
        Err(FileError::Database(_))
    ));
    let admin_metadata = context
        .put_file(
            "reports/summary.json",
            "summary.json",
            "application/json",
            br#"{"status":"ready"}"#,
        )
        .await?;
    println!("{}", admin_metadata.id);
    Ok(())
}
