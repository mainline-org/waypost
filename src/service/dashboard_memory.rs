use super::*;
use crate::{
    domain::{OwnerAuthority, check_id},
    json::{fields, required},
};

impl<D: Database, O: ObjectStore> App<D, O> {
    // A Dashboard operation is attributed to this Space's real owner Actor. It does not
    // impersonate the source of a collection or mint an Agent token behind the user.
    pub(super) async fn dashboard_owner(
        &self,
        request: &Request,
        sid: &str,
        cid: Option<&str>,
    ) -> Result<Principal> {
        check_id(sid)?;
        if request
            .headers
            .get("origin")
            .is_some_and(|o| o != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        if self
            .admin_access(request)
            .await?
            .is_some_and(|scope| scope != sid)
        {
            return Err(Error::not_found());
        }
        let authority = if request.headers.contains_key("authorization") {
            let digest = Sha256::digest(bearer(request)?.as_bytes()).to_vec();
            if digest == Sha256::digest(self.config.admin_secret.as_bytes()).to_vec() {
                OwnerAuthority::Operator
            } else {
                OwnerAuthority::Credential(digest)
            }
        } else {
            OwnerAuthority::Session(
                Sha256::digest(super::dashboard::session_cookie(request)?.as_bytes()).to_vec(),
            )
        };
        let rows = self.db.query(Statement::new("SELECT s.default_actor_id,s.default_consumer_id,a.authority_version,c.view_revision FROM spaces s JOIN actors a ON a.space_id=s.space_id AND a.actor_id=s.default_actor_id JOIN consumers c ON c.space_id=s.space_id AND c.consumer_id=s.default_consumer_id WHERE s.space_id=? AND a.disabled=0 AND c.disabled=0",vec![sid.into()])).await?;
        let row = rows.first().ok_or_else(Error::not_found)?;
        let aid = text(row, "default_actor_id")?;
        let collections = cid.map(|c| vec![c.to_string()]).unwrap_or_default();
        let p = Principal {
            owner_authority: Some(authority),
            space_id: sid.into(),
            actor_id: aid.clone(),
            consumer_id: text(row, "default_consumer_id")?,
            token_id: String::new(),
            actor_version: integer(row, "authority_version")?,
            credential_version: 0,
            view_revision: integer(row, "view_revision")?,
            default_actor_id: aid.clone(),
            default_recipient_id: aid,
            view: "inbox".into(),
            scopes: vec![
                "memory.read".into(),
                "memory.write".into(),
                "blob.write".into(),
            ],
            grants: ResourceGrants {
                memory_read_collection_ids: collections.clone(),
                memory_write_collection_ids: collections,
                ..Default::default()
            },
            filter: FeedFilter::default(),
            grant_fingerprint: fingerprint(&json!({"dashboard_owner":sid,"collection":cid}))?,
            filter_fingerprint: String::new(),
        };
        self.current(&p).await?;
        Ok(p)
    }
    pub(super) fn owner_authority_guard(&self, p: &Principal) -> (String, Vec<Sql>) {
        let mut args = vec![
            p.space_id.clone().into(),
            p.actor_id.clone().into(),
            p.consumer_id.clone().into(),
            p.actor_version.into(),
            p.view_revision.into(),
        ];
        let owner: String = match p.owner_authority.as_ref().expect("owner authority checked") {
            OwnerAuthority::Operator => "1=1".into(),
            OwnerAuthority::Credential(digest) => {
                args.extend([p.space_id.clone().into(), digest.clone().into()]);
                "EXISTS(SELECT 1 FROM space_admin_credentials oc WHERE oc.space_id=? AND oc.credential_digest=? AND oc.revoked_at IS NULL)".into()
            }
            OwnerAuthority::Session(digest) => {
                args.extend([
                    digest.clone().into(),
                    self.clock.now_ms().into(),
                    Sha256::digest(self.config.admin_secret.as_bytes())
                        .to_vec()
                        .into(),
                    p.space_id.clone().into(),
                ]);
                "EXISTS(SELECT 1 FROM admin_sessions os WHERE os.session_digest=? AND os.expires_at>? AND ((os.space_id IS NULL AND os.admin_fingerprint=?) OR (os.space_id=? AND EXISTS(SELECT 1 FROM space_admin_credentials oc WHERE oc.space_id=os.space_id AND oc.credential_digest=os.admin_fingerprint AND oc.revoked_at IS NULL))))".into()
            }
        };
        (
            format!(
                "EXISTS(SELECT 1 FROM spaces s JOIN actors a ON a.space_id=s.space_id AND a.actor_id=s.default_actor_id JOIN consumers c ON c.space_id=s.space_id AND c.consumer_id=s.default_consumer_id WHERE s.space_id=? AND s.default_actor_id=? AND s.default_consumer_id=? AND a.authority_version=? AND c.view_revision=? AND a.disabled=0 AND c.disabled=0 AND {owner})"
            ),
            args,
        )
    }
    pub(super) async fn dashboard_memory_read(
        &self,
        p: &Principal,
        cid: &str,
        value: Value,
    ) -> Result<Value> {
        let tombstones = value.get("include_tombstones") == Some(&Value::Bool(true));
        let mut result = self.memory_read(p, cid, value).await?;
        let revision = result["revision"].as_i64().ok_or_else(Error::storage)?;
        let items = result["items"]
            .as_array_mut()
            .ok_or_else(|| Error::internal("memory page decoding"))?;
        if let (Some(first), Some(last)) = (items.first(), items.last()) {
            let rows=self.db.query(Statement::new("SELECT i.item_id,b.size_bytes,b.sha256,b.content_type FROM memory_revision_items i JOIN blobs b ON b.space_id=i.space_id AND b.blob_id=i.blob_id WHERE i.space_id=? AND i.collection_id=? AND i.revision=? AND i.item_id>=? AND i.item_id<=? AND (?=1 OR i.deleted_in_revision IS NULL) ORDER BY i.item_id",vec![p.space_id.clone().into(),cid.into(),revision.into(),first["item_id"].as_str().ok_or_else(Error::storage)?.into(),last["item_id"].as_str().ok_or_else(Error::storage)?.into(),tombstones.into()])).await?;
            for item in items {
                let row = rows
                    .iter()
                    .find(|r| {
                        r.get("item_id")
                            == Some(&Sql::Text(
                                item["item_id"].as_str().unwrap_or_default().into(),
                            ))
                    })
                    .ok_or_else(|| Error::internal("memory blob metadata missing"))?;
                item["size_bytes"] = json!(integer(row, "size_bytes")?);
                item["sha256"] = json!(hex::encode(blob(row, "sha256")?));
                item["content_type"] = json!(text(row, "content_type")?);
            }
        }
        self.current(p).await?;
        Ok(result)
    }
    async fn dashboard_text_target(
        &self,
        p: &Principal,
        cid: &str,
        expected: u64,
        item_id: &str,
    ) -> Result<(Row, String, Value)> {
        let rows=self.db.query(Statement::new("SELECT * FROM memory_revision_items WHERE space_id=? AND collection_id=? AND revision=? AND item_id=? AND deleted_in_revision IS NULL",vec![p.space_id.clone().into(),cid.into(),(expected as i64).into(),item_id.into()])).await?;
        let item = rows.first().ok_or_else(Error::not_found)?;
        let path = self.open_field(
            &p.space_id,
            &format!("memory/{cid}/{expected}/{item_id}/path"),
            &text(item, "path")?,
        )?;
        let metadata = crate::json::parse(
            self.open_field(
                &p.space_id,
                &format!("memory/{cid}/{expected}/{item_id}/metadata"),
                &text(item, "metadata")?,
            )?
            .as_bytes(),
            self.limits.max_blob_bytes as usize,
        )?;
        let old = self.sealed_blob(p, &text(item, "blob_id")?).await?;
        self.dashboard_text_bytes(&old).await?;
        Ok((old, path, metadata))
    }
    async fn dashboard_text_bytes(&self, row: &Row) -> Result<Vec<u8>> {
        let content_type = text(row, "content_type")?;
        let media = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if !media.starts_with("text/") && media != "application/json" && !media.ends_with("+json") {
            return Err(Error::invalid("Only UTF-8 text documents can be edited."));
        }
        let size = integer(row, "size_bytes")?;
        if size < 0 || size as u64 > self.limits.max_blob_bytes {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Text exceeds deployment limit.",
            ));
        }
        let bytes = self.read_blob_bytes(row).await?;
        std::str::from_utf8(&bytes)
            .map_err(|_| Error::invalid("Only UTF-8 text documents can be edited."))?;
        Ok(bytes)
    }
    async fn dashboard_prepared_text(
        &self,
        p: &Principal,
        cid: &str,
        item: &str,
        expected: u64,
        bid: &str,
    ) -> Result<Value> {
        let row = self.reference_blob(p, bid).await?;
        self.current(p).await?;
        Ok(
            json!({"collection_id":cid,"item_id":item,"expected_revision":expected,"blob_id":bid,"size_bytes":integer(&row,"size_bytes")?,"sha256":hex::encode(blob(&row,"sha256")?),"content_type":text(&row,"content_type")?,"upload_state":"sealed","memory_published":false}),
        )
    }
    pub(super) async fn dashboard_memory_prepare(
        &self,
        p: &Principal,
        cid: &str,
        value: Value,
    ) -> Result<Value> {
        fields(
            &value,
            &["request_id", "expected_revision", "item_id", "text"],
        )?;
        let (expected, item_id, stable) = edit_identity(cid, &value)?;
        let content = required(&value, "text")?;
        if content.len() as u64 > self.limits.max_blob_bytes {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Text exceeds deployment limit.",
            ));
        }
        let operation = "dashboard.memory.prepare";
        let key = format!("dashboard-prepare-{stable}");
        let fp = fingerprint(&json!({"collection_id":cid,"input":value}))?;
        let existing = self.submission(p, operation, &key).await?;
        if let Some(row) = &existing
            && self.existing_result(row, &fp)?.is_some()
        {
            return self
                .dashboard_prepared_text(p, cid, &item_id, expected, &text(row, "resource_id")?)
                .await;
        }
        let collection = self.collection(p, cid).await?;
        if integer(&collection, "current_revision")? != expected as i64 && existing.is_none() {
            return Err(edit_conflict(integer(&collection, "current_revision")?));
        }
        let (old, _, _) = self
            .dashboard_text_target(p, cid, expected, &item_id)
            .await?;
        let content_type = text(&old, "content_type")?;
        let row = self
            .reserve(p, operation, &key, &fp, cid, None, &[])
            .await?;
        let prepared=self.blob_prepare(p,json!({"request_id":format!("dashboard-upload-{stable}"),"content_type":content_type,"size_bytes":content.len(),"sha256":hex::encode(Sha256::digest(content.as_bytes()))})).await?;
        let upload_id = prepared["upload_id"].as_str().ok_or_else(Error::storage)?;
        let upload_row = self.upload(p, upload_id).await?;
        if text(&upload_row, "state")? != "sealed" {
            let mut request = Request::json("PUT", "", "", json!({}));
            request.body = content.into_bytes();
            request.headers.insert("content-type".into(), content_type);
            self.blob_put(p, upload_id, &request).await?;
        }
        let sealed = self
            .blob_commit(
                p,
                upload_id,
                json!({"request_id":format!("dashboard-seal-{stable}")}),
            )
            .await?;
        let bid = sealed["blob_id"].as_str().ok_or_else(Error::storage)?;
        let (authority, auth) = self.authority_guard(p);
        let mut args = vec![
            bid.into(),
            self.clock.now_ms().into(),
            text(&row, "submission_id")?.into(),
        ];
        args.extend(auth);
        self.db.batch(vec![Statement::new(format!("UPDATE submissions SET state='committed',resource_id=?,committed_at=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority}"),args)]).await?;
        self.current(p).await?;
        let done = self
            .submission(p, operation, &key)
            .await?
            .ok_or_else(Error::storage)?;
        if self.existing_result(&done, &fp)?.is_none() {
            return Err(Error::new(
                "SUBMISSION_PENDING",
                "Text preparation is pending; retry with the same request ID.",
            ));
        }
        self.dashboard_prepared_text(p, cid, &item_id, expected, &text(&done, "resource_id")?)
            .await
    }
    pub(super) async fn dashboard_memory_edit(
        &self,
        p: &Principal,
        cid: &str,
        value: Value,
    ) -> Result<Value> {
        fields(
            &value,
            &["request_id", "expected_revision", "item_id", "blob_id"],
        )?;
        let (expected, item_id, stable) = edit_identity(cid, &value)?;
        let bid = required(&value, "blob_id")?;
        check_id(&bid)?;
        let collection = self.collection(p, cid).await?;
        let publish_key = format!("dashboard-edit-{stable}");
        if integer(&collection, "current_revision")? != expected as i64
            && self
                .submission(p, "memory.publish", &publish_key)
                .await?
                .is_none()
        {
            return Err(edit_conflict(integer(&collection, "current_revision")?));
        }
        let (old, path, metadata) = self
            .dashboard_text_target(p, cid, expected, &item_id)
            .await?;
        // A user's prepared text stays owned by that user, never by the collection source.
        // The public final phase cannot reference another Actor's or another Space's blob.
        let replacement = self.reference_blob(p, &bid).await?;
        if text(&replacement, "owner_actor_id")? != p.actor_id {
            return Err(Error::forbidden());
        }
        self.dashboard_text_bytes(&replacement).await?;
        if text(&old, "content_type")? != text(&replacement, "content_type")? {
            return Err(Error::invalid(
                "Replacement content type differs from the original text document.",
            ));
        }
        let mut result=self.memory_publish(p,cid,json!({"request_id":publish_key,"expected_revision":expected,"mode":"delta","items":[{"item_id":item_id,"blob_id":bid,"path":path,"metadata":metadata}]})).await?;
        result["blob_id"] = json!(bid);
        result["item_id"] = json!(item_id);
        Ok(result)
    }
    pub(super) fn dashboard_blob_request(request: &Request) -> bool {
        request.method == "GET"
            && matches!(
                request.path.split('/').collect::<Vec<_>>().as_slice(),
                ["", "admin", "v0", "spaces", _, "blobs", _, "content"]
            )
    }
    pub(super) async fn dashboard_blob_content(&self, request: &Request) -> Result<Response> {
        let parts = request.path.split('/').collect::<Vec<_>>();
        let p = self.dashboard_owner(request, parts[4], None).await?;
        check_id(parts[6])?;
        let row = self.sealed_blob(&p, parts[6]).await?;
        if integer(&row, "size_bytes")? < 0
            || integer(&row, "size_bytes")? as u64 > self.limits.max_blob_bytes
        {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Blob exceeds deployment limit.",
            ));
        }
        let bytes = self.read_blob_bytes(&row).await?;
        self.current(&p).await?;
        Ok(Response {
            status: 200,
            headers: BTreeMap::from([
                ("content-type".into(), text(&row, "content_type")?),
                ("content-disposition".into(), "attachment".into()),
                ("content-length".into(), bytes.len().to_string()),
                ("cache-control".into(), "private, no-store".into()),
                ("x-content-type-options".into(), "nosniff".into()),
            ]),
            body: bytes,
        })
    }
}

fn edit_identity(cid: &str, value: &Value) -> Result<(u64, String, String)> {
    let key = required(value, "request_id")?;
    crate::domain::check_request_id(&key)?;
    let expected = value
        .get("expected_revision")
        .and_then(Value::as_u64)
        .filter(|v| *v < crate::domain::MAX_REVISION)
        .ok_or_else(|| Error::invalid("Invalid expected revision."))?;
    let item = required(value, "item_id")?;
    check_id(&item)?;
    Ok((expected, item, fingerprint(&json!([cid, key]))?))
}
fn edit_conflict(current: i64) -> Error {
    let mut error = Error::new(
        "REVISION_CONFLICT",
        "Memory changed; reload before editing.",
    );
    error.details = json!({"current_revision":current});
    error
}

#[cfg(all(test, feature = "native"))]
#[path = "dashboard_memory_budget_tests.rs"]
mod budget_tests;
