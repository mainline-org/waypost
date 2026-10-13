pub mod server;
use crate::{
    error::{Error, Result},
    storage::{
        ObjectInfo, ObjectStore,
        driver::{Database, Row, Statement, Value},
    },
};
use rusqlite::{
    Connection, params_from_iter,
    types::{Value as SqlValue, ValueRef},
};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone)]
pub struct SqliteDb {
    connection: Rc<RefCell<Connection>>,
}
fn values(parameters: Vec<Value>) -> Vec<SqlValue> {
    parameters
        .into_iter()
        .map(|v| match v {
            Value::Null => SqlValue::Null,
            Value::Integer(n) => SqlValue::Integer(n),
            Value::Text(s) => SqlValue::Text(s),
            Value::Blob(b) => SqlValue::Blob(b),
        })
        .collect()
}
fn database_error(error: rusqlite::Error) -> Error {
    use rusqlite::ErrorCode::*;
    match error.sqlite_error_code() {
        Some(DatabaseBusy | DatabaseLocked | SystemIoFailure | DiskFull | CannotOpen) => {
            Error::storage()
        }
        _ => Error::internal("SQLite statement or schema"),
    }
}
fn execute(connection: &Connection, statement: Statement) -> Result<Vec<Row>> {
    let parameters = values(statement.parameters);
    let mut stmt = connection.prepare(&statement.sql).map_err(database_error)?;
    let names = stmt
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut cursor = stmt
        .query(params_from_iter(parameters))
        .map_err(database_error)?;
    let mut result = Vec::new();
    while let Some(row) = cursor.next().map_err(database_error)? {
        let mut fields = Row::new();
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index).map_err(database_error)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => Value::Integer(n),
                ValueRef::Text(s) => Value::Text(
                    String::from_utf8(s.to_vec())
                        .map_err(|_| Error::internal("SQLite text encoding"))?,
                ),
                ValueRef::Blob(b) => Value::Blob(b.to_vec()),
                ValueRef::Real(_) => return Err(Error::internal("SQLite column type")),
            };
            fields.insert(name.clone(), value);
        }
        result.push(fields);
    }
    Ok(result)
}
impl SqliteDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path).map_err(database_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(database_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
            .map_err(database_error)?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY);",
            )
            .map_err(database_error)?;
        for (index, (_, sql)) in crate::storage::migrations::MIGRATIONS.iter().enumerate() {
            let version = (index + 1) as i64;
            let installed: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=?)",
                    [version],
                    |r| r.get(0),
                )
                .map_err(database_error)?;
            if !installed {
                let tx = connection.transaction().map_err(database_error)?;
                tx.execute_batch(sql).map_err(database_error)?;
                tx.execute(
                    "INSERT INTO schema_migrations(version) VALUES(?)",
                    [version],
                )
                .map_err(database_error)?;
                tx.commit().map_err(database_error)?;
            }
        }
        Ok(Self {
            connection: Rc::new(RefCell::new(connection)),
        })
    }
}
impl Database for SqliteDb {
    async fn query(&self, statement: Statement) -> Result<Vec<Row>> {
        execute(&self.connection.borrow(), statement)
    }
    async fn batch(&self, statements: Vec<Statement>) -> Result<Vec<Vec<Row>>> {
        let mut connection = self.connection.borrow_mut();
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let mut results = Vec::new();
        for statement in statements {
            results.push(execute(&tx, statement)?);
        }
        tx.commit().map_err(database_error)?;
        Ok(results)
    }
}

#[derive(Clone)]
pub struct FileStore {
    root: PathBuf,
    index: Rc<RefCell<Connection>>,
}
impl FileStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        std::fs::create_dir_all(&path).map_err(|_| Error::storage())?;
        let root = path.as_ref().to_path_buf();
        let index = Connection::open(root.join("objects.sqlite")).map_err(|_| Error::storage())?;
        index.execute_batch("PRAGMA synchronous=FULL;CREATE TABLE IF NOT EXISTS objects(object_key TEXT PRIMARY KEY,size INTEGER NOT NULL,sha256 BLOB NOT NULL,content_type TEXT NOT NULL);").map_err(|_|Error::storage())?;
        Ok(Self {
            root,
            index: Rc::new(RefCell::new(index)),
        })
    }
    fn path(&self, key: &str) -> Result<PathBuf> {
        crate::domain::check_id(key)?;
        Ok(self.root.join(key))
    }
}
impl ObjectStore for FileStore {
    async fn create_only(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        expected: &[u8],
    ) -> Result<ObjectInfo> {
        let path = self.path(key)?;
        let digest = Sha256::digest(bytes).to_vec();
        if digest != expected {
            return Err(Error::new(
                "CHECKSUM_MISMATCH",
                "Actual content checksum did not match.",
            ));
        }
        let info = ObjectInfo {
            size: bytes.len() as u64,
            sha256: digest.clone(),
            content_type: content_type.into(),
        };
        self.index.borrow().execute("INSERT OR IGNORE INTO objects(object_key,size,sha256,content_type) VALUES(?,?,?,?)",rusqlite::params![key,bytes.len() as i64,digest,content_type]).map_err(|_|Error::storage())?;
        let indexed: (i64, Vec<u8>, String) = self
            .index
            .borrow()
            .query_row(
                "SELECT size,sha256,content_type FROM objects WHERE object_key=?",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| Error::storage())?;
        if indexed.0 != info.size as i64 || indexed.1 != info.sha256 || indexed.2 != content_type {
            return Err(Error::new(
                "UPLOAD_CONTENT_CONFLICT",
                "Object content is immutable.",
            ));
        }
        if path.exists() {
            return self
                .inspect(key)
                .await?
                .ok_or_else(Error::storage)
                .and_then(|existing| {
                    if existing.size == info.size && existing.sha256 == info.sha256 {
                        Ok(existing)
                    } else {
                        Err(Error::new(
                            "UPLOAD_CONTENT_CONFLICT",
                            "Object content is immutable.",
                        ))
                    }
                });
        }
        let mut random = [0u8; 16];
        getrandom::getrandom(&mut random).map_err(|_| Error::storage())?;
        let temp = self.root.join(format!("tmp_{}", hex::encode(random)));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|_| Error::storage())?;
        let written = (|| -> std::io::Result<()> {
            file.write_all(bytes)?;
            file.sync_all()?;
            std::fs::hard_link(&temp, &path)?;
            std::fs::File::open(&self.root)?.sync_all()
        })();
        let _ = std::fs::remove_file(&temp);
        if let Err(e) = written
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(Error::storage());
        }
        let actual = self.inspect(key).await?.ok_or_else(Error::storage)?;
        if actual.sha256 != info.sha256 || actual.size != info.size {
            return Err(Error::new(
                "UPLOAD_CONTENT_CONFLICT",
                "Object content is immutable.",
            ));
        }
        Ok(actual)
    }
    async fn inspect(&self, key: &str) -> Result<Option<ObjectInfo>> {
        let path = self.path(key)?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(|_| Error::storage())?;
        let content_type: String = self
            .index
            .borrow()
            .query_row(
                "SELECT content_type FROM objects WHERE object_key=?",
                [key],
                |r| r.get(0),
            )
            .map_err(|_| Error::storage())?;
        Ok(Some(ObjectInfo {
            size: bytes.len() as u64,
            sha256: Sha256::digest(&bytes).to_vec(),
            content_type,
        }))
    }
    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        std::fs::read(self.path(key)?).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::not_found()
            } else {
                Error::storage()
            }
        })
    }
    async fn remove(&self, key: &str) -> Result<()> {
        let path = self.path(key)?;
        if path.exists() {
            std::fs::remove_file(path).map_err(|_| Error::storage())?;
            std::fs::File::open(&self.root)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::storage())?;
        }
        self.index
            .borrow()
            .execute("DELETE FROM objects WHERE object_key=?", [key])
            .map_err(|_| Error::storage())?;
        Ok(())
    }
}
