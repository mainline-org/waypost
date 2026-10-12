use super::*;
use crate::{
    domain::{check_id, check_request_id},
    json::{fields, required},
    storage::optional_text,
};

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn blob_prepare(&self, p: &Principal, value: Value) -> Result<Value> {
        p.require("blob.write")?;
        fields(
            &value,
            &[
                "request_id",
                "content_type",
                "size_bytes",
                "sha256",
                "filename",
            ],
        )?;
        let key = required(&value, "request_id")?;
        check_request_id(&key)?;
        let content_type = required(&value, "content_type")?;
        if content_type.len() > 128
            || !content_type.contains('/')
            || content_type.bytes().any(|b| b < 32 || b == 127)
        {
            return Err(Error::invalid("Invalid content type."));
        }
        let size = value
            .get("size_bytes")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::invalid("Invalid size."))?;
        if size > self.limits.max_blob_bytes {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Blob exceeds deployment limit.",
            ));
        }
        let hash = required(&value, "sha256")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::invalid("SHA-256 must be lowercase hexadecimal."));
        }
        let filename = match value.get("filename") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.len() <= 256 && !s.chars().any(char::is_control) => {
                Some(s.clone())
            }
            _ => return Err(Error::invalid("Invalid filename.")),
        };
        let fp = fingerprint(&value)?;
        if let Some(row) = self.submission(p, "blob.prepare", &key).await?
            && self.existing_result(&row, &fp)?.is_some()
        {
            self.current(p).await?;
            return self.upload_result(&row).await;
        }
        let proposed = random_id("upload")?;
        let row = self
            .reserve(p, "blob.prepare", &key, &fp, &proposed, None, &[])
            .await?;
        if text(&row, "state")? == "committed" {
            return self.upload_result(&row).await;
        }
        let uid = text(&row, "resource_id")?;
        let sid = text(&row, "submission_id")?;
        let bid = random_id("blob")?;
        let now = self.clock.now_ms();
        let nonce = random_id("attempt")?;
        let (authority, args) = self.authority_guard(p);
        let mut params = vec![nonce.clone().into(), sid.clone().into()];
        params.extend(args);
        let guard=("EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND attempt_nonce=? AND state='prepared')".into(),vec![sid.clone().into(),nonce.clone().into()]);
        let mut q = vec![Statement::new(
            format!(
                "UPDATE submissions SET state='prepared',attempt_nonce=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority}"
            ),
            params,
        )];
        q.extend(push_rows("uploads","space_id,upload_id,blob_id,producer_id,submission_id,object_key,content_type,size_bytes,sha256,filename,state,created_at,expires_at",vec![vec![p.space_id.clone().into(),uid.clone().into(),bid.clone().into(),p.actor_id.clone().into(),sid.clone().into(),format!("object_{bid}").into(),content_type.into(),(size as i64).into(),hex::decode(hash).map_err(|_|Error::storage())?.into(),filename.into(),"pending".into(),now.into(),(now+3_600_000).into()]],Some(guard)));
        q.push(Statement::new("UPDATE submissions SET state='committed',committed_at=? WHERE submission_id=? AND attempt_nonce=? AND state='prepared'",vec![now.into(),sid.into(),nonce.into()]));
        self.db.batch(q).await?;
        self.current(p).await?;
        let row = self
            .submission(p, "blob.prepare", &key)
            .await?
            .ok_or_else(Error::storage)?;
        if text(&row, "state")? != "committed" {
            return Err(Error::new(
                "SUBMISSION_PENDING",
                "Upload reservation is pending.",
            ));
        }
        self.upload_result(&row).await
    }
    pub(crate) async fn upload_result(&self, row: &Row) -> Result<Value> {
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM uploads WHERE submission_id=?",
                vec![text(row, "submission_id")?.into()],
            ))
            .await?;
        let r = rows.first().ok_or_else(Error::storage)?;
        let id = text(r, "upload_id")?;
        Ok(
            json!({"submission_state":"committed","upload_id":id,"blob_id":text(r,"blob_id")?,"upload_url":format!("{}/v0/uploads/{id}/content",self.config.public_origin),"expires_at":stamp(integer(r,"expires_at")?)?,"required_headers":{"content-type":text(r,"content_type")?,"authorization":"Bearer <current actor token>"}}),
        )
    }
    pub(crate) async fn upload(&self, p: &Principal, id: &str) -> Result<Row> {
        check_id(id)?;
        self.current(p).await?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM uploads WHERE space_id=? AND upload_id=? AND producer_id=?",
                vec![
                    p.space_id.clone().into(),
                    id.into(),
                    p.actor_id.clone().into(),
                ],
            ))
            .await?;
        rows.first().cloned().ok_or_else(Error::not_found)
    }
    pub(crate) async fn blob_put(
        &self,
        p: &Principal,
        id: &str,
        request: &Request,
    ) -> Result<Value> {
        p.require("blob.write")?;
        let row = self.upload(p, id).await?;
        if text(&row, "state")? == "sealed" {
            return Err(Error::new("UPLOAD_SEALED", "Upload is sealed."));
        }
        if integer(&row, "expires_at")? <= self.clock.now_ms() {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        if request.body.len() as u64 > self.limits.max_blob_bytes {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Blob exceeds deployment limit.",
            ));
        }
        let content_type = text(&row, "content_type")?;
        if request.headers.get("content-type") != Some(&content_type)
            || request.body.len() as i64 != integer(&row, "size_bytes")?
        {
            return Err(Error::new(
                "INVALID_BLOB",
                "Size or content type does not match.",
            ));
        }
        let hash = blob(&row, "sha256")?;
        if Sha256::digest(&request.body).as_slice() != hash.as_slice() {
            return Err(Error::new(
                "CHECKSUM_MISMATCH",
                "Actual bytes do not match the expected SHA-256.",
            ));
        }
        let object_key = text(&row, "object_key")?;
        let existing = self.objects.inspect(&object_key).await?;
        self.objects
            .create_only(&object_key, &request.body, &content_type, &hash)
            .await?;
        self.current(p).await?;
        // Late PUTs only create the same immutable bytes; they can never overwrite a sealed object.
        let after = self.upload(p, id).await?;
        if text(&after, "state")? == "sealed" {
            return Err(Error::new("UPLOAD_SEALED", "Upload is sealed."));
        }
        if integer(&after, "expires_at")? <= self.clock.now_ms() {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        Ok(json!({"upload_id":id,"already_uploaded":existing.is_some()}))
    }
    pub(crate) async fn blob_commit(&self, p: &Principal, id: &str, value: Value) -> Result<Value> {
        p.require("blob.write")?;
        fields(&value, &["request_id", "upload_id"])?;
        if value
            .get("upload_id")
            .is_some_and(|v| v.as_str() != Some(id))
        {
            return Err(Error::invalid("Upload ID does not match path."));
        }
        let key = required(&value, "request_id")?;
        check_request_id(&key)?;
        let fp = fingerprint(&json!({"request_id":key,"upload_id":id}))?;
        if let Some(r) = self.submission(p, "blob.commit", &key).await?
            && self.existing_result(&r, &fp)?.is_some()
        {
            self.current(p).await?;
            return self.sealed_result(p, &text(&r, "resource_id")?).await;
        }
        let upload = self.upload(p, id).await?;
        if text(&upload, "state")? != "sealed"
            && integer(&upload, "expires_at")? <= self.clock.now_ms()
        {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        let object = self
            .objects
            .inspect(&text(&upload, "object_key")?)
            .await?
            .ok_or_else(|| Error::new("BLOB_NOT_SEALED", "Upload bytes are missing."))?;
        if object.sha256 != blob(&upload, "sha256")? {
            return Err(Error::new(
                "CHECKSUM_MISMATCH",
                "Stored bytes do not match.",
            ));
        }
        if object.size != integer(&upload, "size_bytes")? as u64
            || object.content_type != text(&upload, "content_type")?
        {
            return Err(Error::new(
                "INVALID_BLOB",
                "Stored size or content type does not match.",
            ));
        }
        let bid = text(&upload, "blob_id")?;
        let row = self
            .reserve(p, "blob.commit", &key, &fp, &bid, None, &[])
            .await?;
        if text(&row, "state")? == "committed" {
            return self.sealed_result(p, &bid).await;
        }
        let sid = text(&row, "submission_id")?;
        let nonce = random_id("attempt")?;
        let now = self.clock.now_ms();
        let (authority, args) = self.authority_guard(p);
        let mut params = vec![nonce.clone().into(), sid.clone().into()];
        params.extend(args);
        params.extend(vec![p.space_id.clone().into(), id.into(), now.into()]);
        let guard=("EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND attempt_nonce=? AND state='prepared')".into(),vec![sid.clone().into(),nonce.clone().into()]);
        let mut q = vec![Statement::new(
            format!(
                "UPDATE submissions SET state='prepared',attempt_nonce=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority} AND EXISTS(SELECT 1 FROM uploads WHERE space_id=? AND upload_id=? AND (state='sealed' OR expires_at>?))"
            ),
            params,
        )];
        q.extend(push_rows("blobs","space_id,blob_id,owner_actor_id,object_key,content_type,size_bytes,sha256,filename,sealed_at",vec![vec![p.space_id.clone().into(),bid.clone().into(),p.actor_id.clone().into(),text(&upload,"object_key")?.into(),object.content_type.into(),(object.size as i64).into(),object.sha256.into(),optional_text(&upload,"filename").into(),now.into()]],Some(guard.clone())));
        let mut params = vec![p.space_id.clone().into(), id.into()];
        params.extend(guard.1.clone());
        q.push(Statement::new(
            format!(
                "UPDATE uploads SET state='sealed' WHERE space_id=? AND upload_id=? AND {}",
                guard.0
            ),
            params,
        ));
        q.push(Statement::new("UPDATE submissions SET state='committed',committed_at=? WHERE submission_id=? AND attempt_nonce=? AND state='prepared'",vec![now.into(),sid.into(),nonce.into()]));
        self.db.batch(q).await?;
        self.current(p).await?;
        let row = self
            .submission(p, "blob.commit", &key)
            .await?
            .ok_or_else(Error::storage)?;
        if text(&row, "state")? != "committed" {
            return Err(Error::new(
                "UPLOAD_EXPIRED",
                "Upload expired before sealing.",
            ));
        }
        self.sealed_result(p, &bid).await
    }
    pub(crate) async fn sealed_blob(&self, p: &Principal, id: &str) -> Result<Row> {
        check_id(id)?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM blobs WHERE space_id=? AND blob_id=?",
                vec![p.space_id.clone().into(), id.into()],
            ))
            .await?;
        rows.first()
            .cloned()
            .ok_or_else(|| Error::new("BLOB_NOT_SEALED", "Blob is not sealed in this Space."))
    }
    pub(crate) async fn reference_blob(&self, p: &Principal, id: &str) -> Result<Row> {
        let row = self.sealed_blob(p, id).await?;
        if text(&row, "owner_actor_id")? != p.actor_id
            && !p.grants.blob_reference_ids.contains(&id.to_string())
        {
            return Err(Error::forbidden());
        }
        self.current(p).await?;
        Ok(row)
    }
    pub(crate) async fn sealed_result(&self, p: &Principal, id: &str) -> Result<Value> {
        let row = self.sealed_blob(p, id).await?;
        Ok(
            json!({"submission_state":"committed","blob_id":id,"size_bytes":integer(&row,"size_bytes")?,"content_type":text(&row,"content_type")?,"sha256":hex::encode(blob(&row,"sha256")?)}),
        )
    }
    pub(crate) async fn readable_blob(&self, p: &Principal, id: &str) -> Result<Row> {
        let row = self.sealed_blob(p, id).await?;
        if text(&row, "owner_actor_id")? == p.actor_id
            || p.grants.blob_reference_ids.contains(&id.to_string())
            || p.has("space.observe")
        {
            self.current(p).await?;
            return Ok(row);
        }
        if p.has("message.read") {
            let rows=self.db.query(Statement::new("SELECT 1 AS readable FROM message_attachments a JOIN messages m ON m.space_id=a.space_id AND m.message_id=a.message_id WHERE a.space_id=? AND a.blob_id=? AND (m.producer_id=? OR EXISTS(SELECT 1 FROM message_recipients r WHERE r.space_id=m.space_id AND r.message_id=m.message_id AND r.actor_id=?)) LIMIT 1",vec![p.space_id.clone().into(),id.into(),p.actor_id.clone().into(),p.actor_id.clone().into()])).await?;
            if !rows.is_empty() {
                self.current(p).await?;
                return Ok(row);
            }
        }
        if p.has("memory.read") {
            let collections=self.db.query(Statement::new("SELECT DISTINCT c.* FROM memory_collections c LEFT JOIN memory_revision_items i ON i.space_id=c.space_id AND i.collection_id=c.collection_id LEFT JOIN memory_revisions r ON r.space_id=c.space_id AND r.collection_id=c.collection_id WHERE c.space_id=? AND (i.blob_id=? OR r.manifest_blob_id=?)",vec![p.space_id.clone().into(),id.into(),id.into()])).await?;
            if collections
                .iter()
                .any(|c| self.can_read_collection(p, c).unwrap_or(false))
            {
                self.current(p).await?;
                return Ok(row);
            }
        }
        Err(Error::not_found())
    }
    pub(crate) async fn blob_get(&self, p: &Principal, id: &str) -> Result<Value> {
        let row = self.readable_blob(p, id).await?;
        Ok(
            json!({"blob_id":id,"size_bytes":integer(&row,"size_bytes")?,"sha256":hex::encode(blob(&row,"sha256")?),"content_type":text(&row,"content_type")?,"filename":optional_text(&row,"filename"),"download_url":format!("{}/v0/blobs/{id}/content",self.config.public_origin),"requires_authorization":true}),
        )
    }
    pub(crate) async fn blob_content(&self, request: &Request) -> Result<Response> {
        if request
            .headers
            .get("origin")
            .is_some_and(|s| s != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        let p = self.authenticate(request).await?;
        let id = request
            .path
            .trim_start_matches("/v0/blobs/")
            .trim_end_matches("/content");
        let row = self.readable_blob(&p, id).await?;
        let bytes = self.objects.read(&text(&row, "object_key")?).await?;
        if bytes.len() as i64 != integer(&row, "size_bytes")?
            || Sha256::digest(&bytes).as_slice() != blob(&row, "sha256")?
        {
            return Err(Error::storage());
        }
        self.current(&p).await?;
        Ok(Response {
            status: 200,
            headers: BTreeMap::from([
                ("content-type".into(), text(&row, "content_type")?),
                ("content-disposition".into(), "attachment".into()),
                ("cache-control".into(), "private, no-store".into()),
                ("x-content-type-options".into(), "nosniff".into()),
            ]),
            body: bytes,
        })
    }
}
