// Workers use single-threaded JS futures.
#![allow(clippy::future_not_send)]
//! Durable archive checkpoints. Active jobs hold temporary signed Play URLs.
use crate::openapi_schema::{ArchivedApk, DownloadInfo, HistoryVersion};
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

#[derive(Deserialize)]
struct JobRow {
    plan: Option<String>,
    hash_state: Option<String>,
    hash_bytes: String,
}

pub struct Job {
    pub plan: Option<DownloadInfo>,
    pub hash_state: Option<String>,
    pub hash_bytes: u64,
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

    pub async fn job(&self, version: i64) -> Result<Job, String> {
        let row = self.db.prepare("SELECT plan,hash_state,hash_bytes FROM version_history WHERE account_key=? AND package=? AND channel=? AND version_code=?")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive job query failed")?.first::<JobRow>(None).await
            .map_err(|_| "Archive job read failed; apply D1 migrations")?
            .ok_or("Archive job disappeared")?;
        Ok(Job {
            plan: row
                .plan
                .map(|json| serde_json::from_str(&json).map_err(|_| "Invalid archive plan"))
                .transpose()?,
            hash_state: row.hash_state,
            hash_bytes: row
                .hash_bytes
                .parse()
                .map_err(|_| "Invalid archive hash offset")?,
        })
    }

    pub async fn set_plan(&self, version: i64, plan: &DownloadInfo) -> Result<(), String> {
        let json = serde_json::to_string(plan).map_err(|_| "Archive plan serialization failed")?;
        self.db.prepare("UPDATE version_history SET plan=? WHERE account_key=? AND package=? AND channel=? AND version_code=? AND state='uploading'")
            .bind_refs(&[D1Type::Text(&json), D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive plan update failed")?.run().await.map_err(|_| "Archive plan update failed")?;
        Ok(())
    }

    pub async fn clear_plan(&self, version: i64) -> Result<(), String> {
        self.db.prepare("UPDATE version_history SET plan=NULL WHERE account_key=? AND package=? AND channel=? AND version_code=?")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive plan refresh failed")?.run().await.map_err(|_| "Archive plan refresh failed")?;
        Ok(())
    }

    pub async fn acquire(&self, version: i64) -> Result<bool, String> {
        let result = self.db.prepare("UPDATE version_history SET lease_until=unixepoch('now')+960 WHERE account_key=? AND package=? AND channel=? AND version_code=? AND state='uploading' AND lease_until<unixepoch('now')")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive lease failed")?.run().await.map_err(|_| "Archive lease failed")?;
        Ok(result
            .meta()
            .map_err(|_| "Archive lease metadata missing")?
            .and_then(|meta| meta.changes)
            == Some(1))
    }

    pub async fn release(&self, version: i64) -> Result<(), String> {
        self.db.prepare("UPDATE version_history SET lease_until=0 WHERE account_key=? AND package=? AND channel=? AND version_code=?")
            .bind_refs(&[D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive lease release failed")?.run().await.map_err(|_| "Archive lease release failed")?;
        Ok(())
    }

    pub async fn save_progress(
        &self,
        version: i64,
        files: &[ArchivedApk],
        hash_state: &str,
        hash_bytes: u64,
    ) -> Result<(), String> {
        let json = serde_json::to_string(files).map_err(|_| "History serialization failed")?;
        let offset = hash_bytes.to_string();
        let result = self.db.prepare("UPDATE version_history SET manifest=?,hash_state=?,hash_bytes=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE account_key=? AND package=? AND channel=? AND version_code=? AND state='uploading'")
            .bind_refs(&[D1Type::Text(&json), D1Type::Text(hash_state), D1Type::Text(&offset), D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version.to_string())])
            .map_err(|_| "Archive checkpoint failed")?.run().await.map_err(|_| "Archive checkpoint failed; uploaded part may already exist")?;
        if result
            .meta()
            .map_err(|_| "Archive checkpoint metadata missing")?
            .and_then(|meta| meta.changes)
            != Some(1)
        {
            return Err("Archive checkpoint did not update the active job".to_string());
        }
        Ok(())
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
        let result = self.db.prepare("UPDATE version_history SET manifest=?,state=?,error=?,plan=CASE WHEN ?='uploading' THEN plan ELSE NULL END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE account_key=? AND package=? AND channel=? AND version_code=? AND state='uploading'")
            .bind_refs(&[D1Type::Text(&json), D1Type::Text(state), error.map_or(D1Type::Null, D1Type::Text), D1Type::Text(state), D1Type::Text(&self.account), D1Type::Text(&self.package), D1Type::Text(&self.channel), D1Type::Text(&version)])
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
