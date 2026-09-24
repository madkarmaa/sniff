// Workers use single-threaded JS futures.
#![allow(clippy::future_not_send)]
//! Durable manifests only: never store AAS tokens or signed Play/Photos URLs.
use crate::openapi_schema::{ArchivedApk, HistoryVersion};
use serde::Deserialize;
use sha2::Digest as _;
use worker::{D1Database, D1Type};

pub struct History {
    db: D1Database,
    account: String,
    package: String,
    channel: String,
}

#[derive(Deserialize)]
struct Row {
    version_code: String,
    state: String,
    manifest: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl Row {
    fn decode(self) -> Result<HistoryVersion, String> {
        Ok(HistoryVersion {
            version_code: self
                .version_code
                .parse()
                .map_err(|_| "Invalid history version")?,
            state: self.state,
            photos: serde_json::from_str(&self.manifest).map_err(|_| "Invalid history manifest")?,
            error: self.error,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len().saturating_mul(2)),
        |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        },
    )
}

impl History {
    pub fn new(db: D1Database, email: &str, package: &str, channel: &str) -> Self {
        Self {
            db,
            account: hex(&sha2::Sha256::digest(
                email.trim().to_ascii_lowercase().as_bytes(),
            )),
            package: package.to_string(),
            channel: channel.to_string(),
        }
    }

    pub async fn get(&self, version: i64) -> Result<Option<HistoryVersion>, String> {
        let version = version.to_string();
        self.db.prepare("SELECT version_code,state,manifest,error,created_at,updated_at FROM version_history WHERE account_key=? AND package=? AND channel=? AND version_code=?")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version)])
            .map_err(|_| "History query failed")?
            .first::<Row>(None).await.map_err(|_| "History read failed; apply D1 migrations")?
            .map(Row::decode).transpose()
    }

    /// Atomic ownership across isolates. An interrupted attempt remains recorded;
    /// never take over and blindly repeat a potentially successful Photos mutation.
    pub async fn claim(&self, version: i64) -> Result<bool, String> {
        let version = version.to_string();
        let result = self.db.prepare("INSERT INTO version_history(account_key,package,channel,version_code,state,manifest) VALUES (?,?,?,?,'uploading','[]') ON CONFLICT DO NOTHING")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version)])
            .map_err(|_| "History claim failed")?.run().await.map_err(|_| "History claim failed")?;
        Ok(result
            .meta()
            .map_err(|_| "History claim metadata missing")?
            .and_then(|meta| meta.changes)
            .is_some_and(|changes| changes == 1))
    }

    pub async fn save(
        &self,
        version: i64,
        files: &[ArchivedApk],
        state: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        let version = version.to_string();
        let json = serde_json::to_string(files).map_err(|_| "History serialization failed")?;
        let result = self.db.prepare("UPDATE version_history SET manifest=?,state=?,error=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE account_key=? AND package=? AND channel=? AND version_code=? AND state='uploading'")
            .bind_refs(&[D1Type::Text(&json), D1Type::Text(state), error.map_or(D1Type::Null, D1Type::Text), D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version)])
            .map_err(|_| "History checkpoint failed")?.run().await.map_err(|_| "History checkpoint failed; uploaded parts may already exist")?;
        if result
            .meta()
            .map_err(|_| "History checkpoint metadata missing")?
            .and_then(|meta| meta.changes)
            != Some(1)
        {
            return Err("History checkpoint did not update the active archive".to_string());
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<HistoryVersion>, String> {
        self.db.prepare("SELECT version_code,state,manifest,error,created_at,updated_at FROM version_history WHERE account_key=? AND package=? AND channel=? ORDER BY created_at DESC LIMIT 100")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel)])
            .map_err(|_| "History query failed")?.all().await.map_err(|_| "History list failed")?
            .results::<Row>().map_err(|_| "History decode failed")?.into_iter().map(Row::decode).collect()
    }
}
