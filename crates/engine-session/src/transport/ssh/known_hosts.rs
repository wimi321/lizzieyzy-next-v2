//! TOFU records are confined to the caller-supplied application data file.
use crate::RemoteTransportError as Error;
use russh::keys::{HashAlg, PublicKey};
use std::{
    fs::OpenOptions,
    io::{Read, Seek, Write},
    path::Path,
};

pub(super) fn verify(path: &Path, host: &str, port: u16, key: &PublicKey) -> Result<(), Error> {
    let unavailable = || Error::KnownHosts {
        path: path.to_owned(),
    };
    let parent = path.parent().ok_or_else(unavailable)?;
    std::fs::create_dir_all(parent).map_err(|_| unavailable())?;
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(unavailable());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| unavailable())?;
    // Serialize concurrent first connections with an OS file lock, not shared
    // in-process state. Drop releases it before any credentials are sent.
    // A different process holding the file must not leave a blocking task alive
    // indefinitely after the caller cancels a connection.
    file.try_lock().map_err(|_| unavailable())?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(|_| unavailable())?;
    let target = if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    };
    let mut recorded = Vec::new();
    for line in text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
    {
        let mut columns = line.split_whitespace();
        let hosts = columns.next().ok_or_else(unavailable)?;
        let algorithm = columns.next().ok_or_else(unavailable)?;
        let encoded = columns.next().ok_or_else(unavailable)?;
        if !hosts.split(',').any(|name| name == target) {
            continue;
        }
        let previous = PublicKey::from_openssh(&format!("{algorithm} {encoded}"))
            .map_err(|_| unavailable())?;
        if previous.key_data() == key.key_data() {
            return Ok(());
        }
        recorded.push(previous.fingerprint(HashAlg::Sha256).to_string());
    }
    if !recorded.is_empty() {
        return Err(Error::HostKeyChanged {
            old_fingerprint: recorded.join(", "),
            new_fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
            path: path.to_owned(),
        });
    }
    let encoded = key.to_openssh().map_err(|_| unavailable())?;
    file.seek(std::io::SeekFrom::End(0))
        .map_err(|_| unavailable())?;
    if !text.is_empty() && !text.ends_with('\n') {
        file.write_all(b"\n").map_err(|_| unavailable())?;
    }
    writeln!(file, "{target} {encoded}").map_err(|_| unavailable())?;
    file.sync_all().map_err(|_| unavailable())?;
    Ok(())
}
