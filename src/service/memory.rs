use super::*;
use crate::{
    domain::{check_id, check_request_id},
    json::fields,
    storage::driver::optional_text,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct MemoryItem {
    item_id: String,
    blob_id: String,
    path: String,
    #[serde(default = "empty_metadata")]
    metadata: Value,
}
fn empty_metadata() -> Value {
    json!({})
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct VersionItem {
    #[serde(flatten)]
    item: MemoryItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_in_revision: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publish {
    request_id: String,
    collection_id: Option<String>,
    expected_revision: u64,
    mode: String,
    items: Option<Vec<MemoryItem>>,
    delete_ids: Option<Vec<String>>,
    manifest_blob_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Changes {
    #[serde(default)]
    items: Vec<MemoryItem>,
    #[serde(default)]
    delete_ids: Vec<String>,
}
fn validate_changes(changes: &Changes, mode: &str, max: usize) -> Result<()> {
    if changes.items.len() + changes.delete_ids.len() > max {
        return Err(Error::new("RESOURCE_LIMIT", "Too many memory changes."));
    }
    if !["snapshot", "delta"].contains(&mode) {
        return Err(Error::invalid("Invalid memory mode."));
    }
    if mode == "snapshot" && !changes.delete_ids.is_empty() {
        return Err(Error::invalid("Snapshots cannot contain delete IDs."));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for item in &changes.items {
        check_id(&item.item_id)?;
        check_id(&item.blob_id)?;
        if !ids.insert(&item.item_id) {
            return Err(Error::invalid("Duplicate memory item ID."));
        }
        if item.path.len() > 1024
            || item.path.contains('\\')
            || item.path.contains(':')
            || item.path.chars().any(char::is_control)
            || item
                .path
                .split('/')
                .any(|s| s.is_empty() || s == ".." || s == ".")
        {
            return Err(Error::invalid("Invalid relative memory path."));
        }
        if !paths.insert(&item.path) {
            return Err(Error::invalid("Duplicate memory path."));
        }
        if !item.metadata.is_object() {
            return Err(Error::invalid("Memory metadata must be an object."));
        }
    }
    for id in &changes.delete_ids {
        check_id(id)?;
        if !ids.insert(id) {
            return Err(Error::invalid("Duplicate or intersecting delete ID."));
        }
    }
    Ok(())
}
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn collection(&self, p: &Principal, id: &str) -> Result<Row> {
        check_id(id)?;
        self.current(p).await?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM memory_collections WHERE space_id=? AND collection_id=?",
                vec![p.space_id.clone().into(), id.into()],
            ))
            .await?;
        rows.first().cloned().ok_or_else(Error::not_found)
    }
    pub(crate) fn can_read_collection(&self, p: &Principal, row: &Row) -> Result<bool> {
        Ok(p.has("memory.read")
            && (text(row, "source_actor_id")? == p.actor_id
                || p.grants
                    .memory_read_collection_ids
                    .contains(&text(row, "collection_id")?)))
    }
    async fn revision_items(
        &self,
        space: &str,
        cid: &str,
        revision: i64,
    ) -> Result<BTreeMap<String, VersionItem>> {
        let rows=self.db.query(Statement::new("SELECT * FROM memory_revision_items WHERE space_id=? AND collection_id=? AND revision=? ORDER BY item_id",vec![space.into(),cid.into(),revision.into()])).await?;
        let mut items = BTreeMap::new();
        for row in rows {
            let id = text(&row, "item_id")?;
            let deleted = match row.get("deleted_in_revision") {
                Some(Sql::Null) | None => None,
                _ => Some(integer(&row, "deleted_in_revision")?),
            };
            items.insert(
                id.clone(),
                VersionItem {
                    item: MemoryItem {
                        item_id: id.clone(),
                        blob_id: text(&row, "blob_id")?,
                        path: self.open_field(
                            space,
                            &format!("memory/{cid}/{revision}/{id}/path"),
                            &text(&row, "path")?,
                        )?,
                        metadata: crate::json::parse(
                            self.open_field(
                                space,
                                &format!("memory/{cid}/{revision}/{id}/metadata"),
                                &text(&row, "metadata")?,
                            )?
                            .as_bytes(),
                            self.limits.max_blob_bytes as usize,
                        )?,
                    },
                    deleted_in_revision: deleted,
                },
            );
        }
        Ok(items)
    }
    pub(crate) async fn memory_publish(
        &self,
        p: &Principal,
        cid: &str,
        value: Value,
    ) -> Result<Value> {
        p.require("memory.write")?;
        let input: Publish = crate::json::decode(value.clone())?;
        check_request_id(&input.request_id)?;
        if input.collection_id.as_ref().is_some_and(|v| v != cid) {
            return Err(Error::invalid("Collection does not match path."));
        }
        if input.expected_revision > 9_007_199_254_740_990 {
            return Err(Error::invalid("Invalid expected revision."));
        }
        let collection = self.collection(p, cid).await?;
        if text(&collection, "source_actor_id")? != p.actor_id
            && !p
                .grants
                .memory_write_collection_ids
                .contains(&cid.to_string())
        {
            return Err(Error::forbidden());
        }
        let mut normalized = value;
        normalized
            .as_object_mut()
            .ok_or_else(|| Error::invalid("Expected object."))?
            .insert("collection_id".into(), json!(cid));
        let fp = fingerprint(&normalized)?;
        if let Some(row) = self
            .submission(p, "memory.publish", &input.request_id)
            .await?
            && let Some(result) = self.existing_result(&row, &fp)?
        {
            self.current(p).await?;
            return Ok(result);
        }
        let from_manifest = input.manifest_blob_id.is_some();
        if from_manifest && (input.items.is_some() || input.delete_ids.is_some()) {
            return Err(Error::invalid(
                "Manifest and inline changes are mutually exclusive.",
            ));
        }
        let mut changes = if let Some(id) = &input.manifest_blob_id {
            let r = self.reference_blob(p, id).await?;
            let bytes = self.read_blob_bytes(&r).await?;
            crate::json::decode::<Changes>(crate::json::parse(
                &bytes,
                self.limits.max_blob_bytes as usize,
            )?)?
        } else {
            Changes {
                items: input.items.unwrap_or_default(),
                delete_ids: input.delete_ids.unwrap_or_default(),
            }
        };
        validate_changes(
            &changes,
            &input.mode,
            if from_manifest {
                self.limits.max_manifest_memory_changes
            } else {
                self.limits.max_inline_memory_changes
            },
        )?;
        for item in &mut changes.items {
            self.reference_blob(p, &item.blob_id).await?;
            // Compare the same JCS semantic representation that is persisted in historical rows.
            item.metadata = crate::json::parse(
                &crate::json::canonical(&item.metadata)?,
                self.limits.max_blob_bytes as usize,
            )?;
        }
        let row = self
            .reserve(p, "memory.publish", &input.request_id, &fp, cid, None, &[])
            .await?;
        if let Some(result) = self.existing_result(&row, &fp)? {
            return Ok(result);
        }
        let expected = input.expected_revision as i64;
        let head = integer(&collection, "current_revision")?;
        if head != expected {
            self.memory_conflict(p, &row, cid, expected).await?;
            return Err(Error::new("SUBMISSION_PENDING", "Retry submission status."));
        }
        let old = self.revision_items(&p.space_id, cid, expected).await?;
        let mut next = old.clone();
        let next_rev = expected + 1;
        if input.mode == "snapshot" {
            let present = changes
                .items
                .iter()
                .map(|i| i.item_id.as_str())
                .collect::<BTreeSet<_>>();
            for item in next.values_mut() {
                if item.deleted_in_revision.is_none()
                    && !present.contains(item.item.item_id.as_str())
                {
                    item.deleted_in_revision = Some(next_rev)
                }
            }
        }
        for id in changes.delete_ids {
            if let Some(item) = next.get_mut(&id)
                && item.deleted_in_revision.is_none()
            {
                item.deleted_in_revision = Some(next_rev)
            }
        }
        for item in changes.items {
            next.insert(
                item.item_id.clone(),
                VersionItem {
                    item,
                    deleted_in_revision: None,
                },
            );
        }
        if next.len() > self.limits.max_collection_items {
            return self
                .fail_submission(
                    p,
                    &row,
                    Error::new("RESOURCE_LIMIT", "Collection exceeds deployment limit."),
                    None,
                )
                .await;
        }
        let mut paths = BTreeSet::new();
        for item in next.values().filter(|i| i.deleted_in_revision.is_none()) {
            if !paths.insert(&item.item.path) {
                return self
                    .fail_submission(
                        p,
                        &row,
                        Error::invalid("Resulting memory paths conflict."),
                        None,
                    )
                    .await;
            }
        }
        let changed = next != old;
        let revision = if changed { next_rev } else { expected };
        let sid = text(&row, "submission_id")?;
        let nonce = random_id("attempt")?;
        let now = self.clock.now_ms();
        let manifest_id = format!("manifest_{sid}");
        let bytes = crate::json::canonical(
            &json!({"collection_id":cid,"revision":revision,"items":next.values().collect::<Vec<_>>()}),
        )?;
        if bytes.len() as u64 > self.limits.max_blob_bytes {
            return self
                .fail_submission(
                    p,
                    &row,
                    Error::new(
                        "RESOURCE_LIMIT",
                        "Memory manifest exceeds deployment limit.",
                    ),
                    None,
                )
                .await;
        }
        let object = if changed {
            self.journal_object(
                p,
                &sid,
                &self.storage_key(&p.space_id, &format!("object_{manifest_id}")),
            )
            .await?;
            Some(
                self.object_create(
                    &p.space_id,
                    &self.storage_key(&p.space_id, &format!("object_{manifest_id}")),
                    &bytes,
                    "application/json",
                    &Sha256::digest(&bytes),
                )
                .await?,
            )
        } else {
            None
        };
        let (authority, args) = self.authority_guard(p);
        let mut params = vec![nonce.clone().into(), sid.clone().into()];
        params.extend(args);
        params.extend(vec![p.space_id.clone().into(), cid.into(), expected.into()]);
        let guard=("EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND attempt_nonce=? AND state='prepared')".into(),vec![sid.clone().into(),nonce.clone().into()]);
        let mut q = vec![Statement::new(
            format!(
                "UPDATE submissions SET state='prepared',attempt_nonce=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority} AND EXISTS(SELECT 1 FROM memory_collections WHERE space_id=? AND collection_id=? AND current_revision=?)"
            ),
            params,
        )];
        if let Some(object) = object {
            q.extend(push_rows("blobs","space_id,blob_id,owner_actor_id,object_key,content_type,size_bytes,sha256,sealed_at",vec![vec![p.space_id.clone().into(),manifest_id.clone().into(),text(&collection,"source_actor_id")?.into(),self.storage_key(&p.space_id,&format!("object_{manifest_id}")).into(),"application/json".into(),(object.size as i64).into(),object.sha256.into(),now.into()]],Some(guard.clone())));
            q.extend(push_rows("memory_revisions","space_id,collection_id,revision,parent_revision,manifest_blob_id,publisher_actor_id,submission_id,committed_at",vec![vec![p.space_id.clone().into(),cid.into(),revision.into(),expected.into(),manifest_id.clone().into(),p.actor_id.clone().into(),sid.clone().into(),now.into()]],Some(guard.clone())));
            let mut items = Vec::new();
            for item in next.values() {
                items.push(vec![
                    p.space_id.clone().into(),
                    cid.into(),
                    revision.into(),
                    item.item.item_id.clone().into(),
                    item.item.blob_id.clone().into(),
                    self.seal_field(
                        &p.space_id,
                        &format!("memory/{cid}/{revision}/{}/path", item.item.item_id),
                        &item.item.path,
                    )?
                    .into(),
                    self.seal_field(
                        &p.space_id,
                        &format!("memory/{cid}/{revision}/{}/metadata", item.item.item_id),
                        &String::from_utf8(crate::json::canonical(&item.item.metadata)?)
                            .map_err(|_| Error::internal("memory metadata encoding"))?,
                    )?
                    .into(),
                    item.deleted_in_revision.into(),
                ]);
            }
            q.extend(push_rows(
                "memory_revision_items",
                "space_id,collection_id,revision,item_id,blob_id,path,metadata,deleted_in_revision",
                items,
                Some(guard.clone()),
            ));
            let mut params = vec![
                revision.into(),
                p.space_id.clone().into(),
                cid.into(),
                expected.into(),
            ];
            params.extend(guard.1.clone());
            q.push(Statement::new(format!("UPDATE memory_collections SET current_revision=? WHERE space_id=? AND collection_id=? AND current_revision=? AND {}",guard.0),params));
            q.extend(push_rows(
                "feed_items",
                "item_id,space_id,item_type,resource_id,resource_version,producer_id,committed_at",
                vec![vec![
                    random_id("feed")?.into(),
                    p.space_id.clone().into(),
                    "memory_revision".into(),
                    cid.into(),
                    revision.into(),
                    p.actor_id.clone().into(),
                    now.into(),
                ]],
                Some(guard.clone()),
            ));
        }
        let mut params = vec![
            now.into(),
            revision.into(),
            changed.into(),
            if changed { Some(manifest_id) } else { None }.into(),
            sid.clone().into(),
            nonce.into(),
        ];
        // Both changed and no-change submissions reach one guarded metadata commit point.
        q.push(Statement::new("UPDATE submissions SET state='committed',committed_at=?,result_revision=?,result_changed=?,result_manifest_blob_id=? WHERE submission_id=? AND attempt_nonce=? AND state='prepared'",std::mem::take(&mut params)));
        self.db.batch(q).await?;
        self.current(p).await?;
        let row = self
            .submission(p, "memory.publish", &input.request_id)
            .await?
            .ok_or_else(Error::storage)?;
        if let Some(result) = self.existing_result(&row, &fp)? {
            return Ok(result);
        }
        self.memory_conflict(p, &row, cid, expected).await?;
        Err(Error::new(
            "SUBMISSION_PENDING",
            "Memory submission is pending.",
        ))
    }
    async fn memory_conflict(
        &self,
        p: &Principal,
        row: &Row,
        cid: &str,
        expected: i64,
    ) -> Result<()> {
        let (authority, args) = self.authority_guard(p);
        let mut params = vec![
            p.space_id.clone().into(),
            cid.into(),
            text(row, "submission_id")?.into(),
        ];
        params.extend(args);
        params.extend(vec![p.space_id.clone().into(), cid.into(), expected.into()]);
        self.db.batch(vec![Statement::new(format!("UPDATE submissions SET state='failed',error_code='REVISION_CONFLICT',error_current_revision=(SELECT current_revision FROM memory_collections WHERE space_id=? AND collection_id=?) WHERE submission_id=? AND state IN('reserved','prepared') AND {authority} AND EXISTS(SELECT 1 FROM memory_collections WHERE space_id=? AND collection_id=? AND current_revision<>?)"),params)]).await?;
        self.current(p).await?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM submissions WHERE submission_id=?",
                vec![text(row, "submission_id")?.into()],
            ))
            .await?;
        if let Some(r) = rows.first()
            && text(r, "state")? == "failed"
        {
            return Err(super::mailbox::recorded_error(r)?);
        }
        Ok(())
    }
    pub(crate) async fn memory_read(
        &self,
        p: &Principal,
        cid: &str,
        value: Value,
    ) -> Result<Value> {
        fields(
            &value,
            &[
                "collection_id",
                "revision",
                "limit",
                "cursor",
                "include_tombstones",
            ],
        )?;
        if value
            .get("collection_id")
            .is_some_and(|v| v.as_str() != Some(cid))
        {
            return Err(Error::invalid("Collection does not match path."));
        }
        let collection = self.collection(p, cid).await?;
        if !self.can_read_collection(p, &collection)? {
            return Err(Error::not_found());
        }
        let tomb = match value.get("include_tombstones") {
            None => false,
            Some(Value::Bool(v)) => *v,
            _ => return Err(Error::invalid("Invalid tombstone flag.")),
        };
        let requested = match value.get("revision") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|n| *n <= 9_007_199_254_740_991)
                    .ok_or_else(|| Error::invalid("Invalid revision."))? as i64,
            ),
        };
        let mut revision = requested.unwrap_or(integer(&collection, "current_revision")?);
        let mut after = String::new();
        if let Some(cursor) = value.get("cursor").filter(|v| !v.is_null()) {
            let payload = self.verify_cursor(
                cursor
                    .as_str()
                    .ok_or_else(|| Error::new("INVALID_CURSOR", "Invalid cursor."))?,
            )?;
            let f = payload.split('|').collect::<Vec<_>>();
            if f.len() != 8 || f[0] != "m1" {
                return Err(Error::new("INVALID_CURSOR", "Invalid cursor."));
            }
            if f[1] != p.space_id
                || f[2] != p.actor_id
                || f[3] != cid
                || f[5] != tomb.to_string()
                || f[6] != p.grant_fingerprint
            {
                return Err(Error::new(
                    "CURSOR_SCOPE_CHANGED",
                    "Memory cursor scope changed.",
                ));
            }
            revision = f[4]
                .parse()
                .map_err(|_| Error::new("INVALID_CURSOR", "Invalid revision in cursor."))?;
            if requested.is_some_and(|r| r != revision) {
                return Err(Error::new(
                    "INVALID_CURSOR",
                    "Explicit revision differs from cursor.",
                ));
            }
            after = f[7].into();
        }
        let revision_row = if revision > 0 {
            let rows=self.db.query(Statement::new("SELECT * FROM memory_revisions WHERE space_id=? AND collection_id=? AND revision=?",vec![p.space_id.clone().into(),cid.into(),revision.into()])).await?;
            Some(rows.first().cloned().ok_or_else(Error::not_found)?)
        } else {
            None
        };
        let limit = super::mailbox::page_limit(&value, self.limits.max_page_size)?;
        let rows=self.db.query(Statement::new("SELECT * FROM memory_revision_items WHERE space_id=? AND collection_id=? AND revision=? AND item_id>? AND (?=1 OR deleted_in_revision IS NULL) ORDER BY item_id LIMIT ?",vec![p.space_id.clone().into(),cid.into(),revision.into(),after.clone().into(),tomb.into(),((limit+1) as i64).into()])).await?;
        let more = rows.len() > limit;
        let mut items = Vec::new();
        for row in rows.iter().take(limit) {
            after = text(row, "item_id")?;
            let mut item = json!({"item_id":after,"blob_id":text(row,"blob_id")?,"path":self.open_field(&p.space_id,&format!("memory/{cid}/{revision}/{after}/path"),&text(row,"path")?)?,"metadata":crate::json::parse(self.open_field(&p.space_id,&format!("memory/{cid}/{revision}/{after}/metadata"),&text(row,"metadata")?)?.as_bytes(),self.limits.max_blob_bytes as usize)?});
            match row.get("deleted_in_revision") {
                Some(Sql::Integer(n)) => item["deleted_in_revision"] = json!(n),
                Some(Sql::Null) | None => {}
                _ => return Err(Error::internal("decode memory tombstone revision")),
            }
            items.push(item);
        }
        let counts=self.db.query(Statement::new("SELECT COUNT(*) AS total,COUNT(CASE WHEN deleted_in_revision IS NULL THEN 1 END) AS active FROM memory_revision_items WHERE space_id=? AND collection_id=? AND revision=?",vec![p.space_id.clone().into(),cid.into(),revision.into()])).await?;
        let counts = counts.first().ok_or_else(Error::storage)?;
        self.current(p).await?;
        let cursor = self.sign_cursor(&format!(
            "m1|{}|{}|{}|{}|{}|{}|{}",
            p.space_id, p.actor_id, cid, revision, tomb, p.grant_fingerprint, after
        ))?;
        Ok(
            json!({"collection_id":cid,"source_actor_id":text(&collection,"source_actor_id")?,"scope":text(&collection,"scope")?,"revision":revision,"manifest_blob_id":revision_row.as_ref().and_then(|r|optional_text(r,"manifest_blob_id")),"items":items,"has_more":more,"next_cursor":cursor,"active_items":integer(counts,"active")?,"tombstones":integer(counts,"total")?-integer(counts,"active")?}),
        )
    }
    pub(crate) async fn admin_collection(&self, sid: &str, cid: &str) -> Result<Value> {
        check_id(cid)?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM memory_collections WHERE space_id=? AND collection_id=?",
                vec![sid.into(), cid.into()],
            ))
            .await?;
        let r = rows.first().ok_or_else(Error::not_found)?;
        Ok(
            json!({"space_id":sid,"collection_id":cid,"source_actor_id":text(r,"source_actor_id")?,"scope":text(r,"scope")?,"display_name":text(r,"display_name")?,"current_revision":integer(r,"current_revision")?}),
        )
    }
}
