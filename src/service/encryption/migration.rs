use super::*;

impl<D: Database, O: ObjectStore> App<D, O> {
    // Bounded, operator-only migration. Immutable source objects stay in place until
    // metadata moves atomically; cleanup later removes their unreferenced bytes.
    pub(crate) async fn encrypt_existing(&self, body: Value) -> Result<Value> {
        crate::json::fields(&body, &["kind", "dry_run", "limit", "cursor"])?;
        let encryption = self
            .encryption
            .as_ref()
            .ok_or_else(|| Error::invalid("Configure encryption before migrating."))?;
        let kind = crate::json::required(&body, "kind")?;
        let dry_run = body
            .get("dry_run")
            .and_then(Value::as_bool)
            .ok_or_else(|| Error::invalid("dry_run must be a boolean."))?;
        let limit = super::mailbox::page_limit(&body, self.limits.max_page_size.min(4))?;
        let mut after = String::new();
        if let Some(c) = body.get("cursor").filter(|v| !v.is_null()) {
            let payload = self.verify_cursor(
                c.as_str()
                    .ok_or_else(|| Error::invalid("Invalid migration cursor."))?,
            )?;
            let values: Vec<String> = serde_json::from_str(&payload)
                .map_err(|_| Error::invalid("Invalid migration cursor."))?;
            if values.len() != 3 || values[0] != "encrypt1" || values[1] != kind {
                return Err(Error::invalid("Migration cursor scope differs."));
            }
            after = values[2].clone();
        }
        let (table, page_key, condition) = match kind.as_str() {
            "objects" => ("submissions", "object_key", ""),
            "messages" => ("messages", "message_id", ""),
            "uploads" => ("uploads", "upload_id", " AND filename IS NOT NULL"),
            "blobs" => ("blobs", "blob_id", " AND filename IS NOT NULL"),
            "memory_items" => (
                "memory_revision_items",
                "space_id||'|'||collection_id||'|'||printf('%020d',revision)||'|'||item_id",
                "",
            ),
            _ => return Err(Error::invalid("Unknown encryption migration kind.")),
        };
        let query = if kind == "objects" {
            "SELECT space_id,object_key,object_key AS page_key FROM (SELECT space_id,object_key FROM blobs UNION SELECT space_id,body_ref AS object_key FROM submissions WHERE body_ref IS NOT NULL AND state IN('committed','failed')) WHERE object_key>? ORDER BY object_key LIMIT ?".to_owned()
        } else {
            format!(
                "SELECT *,{page_key} AS page_key FROM {table} WHERE {page_key}>?{condition} ORDER BY {page_key} LIMIT ?"
            )
        };
        let rows = self
            .db
            .query(Statement::new(
                query,
                vec![after.into(), ((limit + 1) as i64).into()],
            ))
            .await?;
        let mut converted = 0;
        let mut candidates = 0;
        for row in rows.iter().take(limit) {
            let space = text(row, "space_id")?;
            if kind == "objects" {
                let source = text(row, "object_key")?;
                let stored = self.objects.read(&source).await?;
                let (plain, ct, current) = if stored.starts_with(MAGIC) {
                    let (plain, ct) = encryption.open(&space, &source, &stored)?;
                    let len = u32::from_be_bytes(
                        stored[8..12]
                            .try_into()
                            .map_err(|_| Error::internal("migration header"))?,
                    ) as usize;
                    let header: Header = serde_json::from_slice(&stored[12..12 + len])
                        .map_err(|_| Error::internal("migration header"))?;
                    (plain, ct, header.kid == encryption.active)
                } else {
                    let info = self
                        .objects
                        .inspect(&source)
                        .await?
                        .ok_or_else(Error::storage)?;
                    (stored, info.content_type, false)
                };
                if current {
                    continue;
                }
                let expected=self.db.query(Statement::new("SELECT body_size AS size_bytes,body_sha256 AS sha256,'application/json' AS content_type FROM submissions WHERE space_id=? AND body_ref=? UNION ALL SELECT size_bytes,sha256,content_type FROM blobs WHERE space_id=? AND object_key=? LIMIT 1",vec![space.clone().into(),source.clone().into(),space.clone().into(),source.clone().into()])).await?;
                crate::storage::rows::verify_object(
                    expected
                        .first()
                        .ok_or_else(|| Error::internal("migration object metadata"))?,
                    &ObjectInfo {
                        size: plain.len() as u64,
                        sha256: Sha256::digest(&plain).to_vec(),
                        content_type: ct.clone(),
                    },
                )?;
                candidates += 1;
                if dry_run {
                    continue;
                }
                let now = self.clock.now_ms();
                let lease = random_id("migration")?;
                let target = self.storage_key(&space, &random_id("encrypted")?);
                let owners=self.db.query(Statement::new("SELECT submission_id FROM object_writes WHERE object_key=? UNION SELECT submission_id FROM uploads WHERE object_key=? UNION SELECT submission_id FROM submissions WHERE body_ref=? UNION SELECT r.submission_id FROM memory_revisions r JOIN blobs b ON b.space_id=r.space_id AND b.blob_id=r.manifest_blob_id WHERE b.object_key=? UNION SELECT m.submission_id FROM messages m JOIN message_attachments a ON a.space_id=m.space_id AND a.message_id=m.message_id JOIN blobs b ON b.space_id=a.space_id AND b.blob_id=a.blob_id WHERE b.object_key=? LIMIT 1",vec![source.clone().into();5])).await?;
                let sid = text(
                    owners
                        .first()
                        .ok_or_else(|| Error::internal("migration object owner"))?,
                    "submission_id",
                )?;
                self.db.batch(vec![Statement::new("INSERT INTO object_writes(object_key,submission_id,created_at,lease_nonce,lease_until) VALUES(?,?,?,?,?)",vec![target.clone().into(),sid.into(),now.into(),lease.clone().into(),(now+300_000).into()])]).await?;
                self.object_create(&space, &target, &plain, &ct, &Sha256::digest(&plain))
                    .await?;
                let guard = "EXISTS(SELECT 1 FROM object_writes WHERE object_key=? AND state='active' AND lease_nonce=?)";
                let mut statements = vec![];
                for (table, column) in [
                    ("submissions", "body_ref"),
                    ("messages", "body_ref"),
                    ("blobs", "object_key"),
                    ("uploads", "object_key"),
                ] {
                    statements.push(Statement::new(format!("UPDATE {table} SET {column}=? WHERE space_id=? AND {column}=? AND {guard}"),vec![target.clone().into(),space.clone().into(),source.clone().into(),target.clone().into(),lease.clone().into()]));
                }
                statements.push(Statement::new("INSERT INTO object_writes(object_key,submission_id,created_at) SELECT ?,submission_id,? FROM object_writes WHERE object_key=? ON CONFLICT(object_key) DO NOTHING",vec![source.into(),now.into(),target.clone().into()]));
                let target_check = target.clone();
                statements.push(Statement::new("UPDATE object_writes SET lease_until=NULL,lease_nonce=NULL WHERE object_key=? AND state='active' AND lease_nonce=?",vec![target.into(),lease.into()]));
                self.db.batch(statements).await?;
                let published=self.db.query(Statement::new("SELECT 1 AS published WHERE EXISTS(SELECT 1 FROM submissions WHERE body_ref=?) OR EXISTS(SELECT 1 FROM blobs WHERE object_key=?)",vec![target_check.clone().into(),target_check.into()])).await?;
                if published.is_empty() {
                    return Err(Error::new(
                        "SUBMISSION_PENDING",
                        "Migration lost its object lease; retry.",
                    ));
                }
                converted += 1;
            } else {
                let identity = text(row, "page_key")?;
                let columns = match kind.as_str() {
                    "messages" => vec!["subject"],
                    "memory_items" => vec!["path", "metadata"],
                    _ => vec!["filename"],
                };
                let mut statements = vec![];
                for column in columns {
                    let context = match kind.as_str() {
                        "messages" => format!("message/{identity}/subject"),
                        "uploads" | "blobs" => format!("blob/{}/filename", text(row, "blob_id")?),
                        _ => format!(
                            "memory/{}/{}/{}/{column}",
                            text(row, "collection_id")?,
                            integer(row, "revision")?,
                            text(row, "item_id")?
                        ),
                    };
                    let stored = text(row, column)?;
                    let plain = if let Some(encoded) = stored.strip_prefix("wpenc1:") {
                        let bytes = URL_SAFE_NO_PAD
                            .decode(encoded)
                            .map_err(|_| Error::internal("migration field"))?;
                        let (plain, _) = encryption.open(&space, &context, &bytes)?;
                        let len = u32::from_be_bytes(
                            bytes[8..12]
                                .try_into()
                                .map_err(|_| Error::internal("migration field header"))?,
                        ) as usize;
                        let header: Header = serde_json::from_slice(&bytes[12..12 + len])
                            .map_err(|_| Error::internal("migration field header"))?;
                        if header.kid == encryption.active {
                            continue;
                        }
                        String::from_utf8(plain)
                            .map_err(|_| Error::internal("migration field text"))?
                    } else {
                        stored.clone()
                    };
                    candidates += 1;
                    if dry_run {
                        continue;
                    }
                    statements.push(Statement::new(format!("UPDATE {table} SET {column}=? WHERE space_id=? AND {page_key}=? AND {column}=?"),vec![self.seal_field(&space,&context,&plain)?.into(),space.clone().into(),identity.clone().into(),stored.into()]));
                }
                if !statements.is_empty() {
                    self.db.batch(statements).await?;
                    converted += 1;
                }
            }
        }
        let next = rows
            .iter()
            .take(limit)
            .next_back()
            .map(|r| self.sign_cursor(&json!(["encrypt1", kind, text(r, "page_key")?]).to_string()))
            .transpose()?;
        Ok(
            json!({"dry_run":dry_run,"candidate_count":candidates,"converted":converted,"has_more":rows.len()>limit,"next_cursor":next}),
        )
    }
}
