use std::path::{Path, PathBuf};

use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub async fn write_atomic(destination: &Path, ciphertext: &[u8]) -> std::io::Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "vault destination must have a parent directory",
        )
    })?;
    fs::create_dir_all(parent).await?;

    let temporary_path = temporary_path(parent);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary_path)
        .await?;
    file.write_all(ciphertext).await?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    fs::rename(&temporary_path, destination).await
}

fn temporary_path(parent: &Path) -> PathBuf {
    parent.join(format!(".{}.tmp", Uuid::new_v4()))
}
