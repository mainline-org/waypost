use super::*;
use crate::{
    domain::check_id,
    json::{fields, required},
};
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) fn dashboard_asset(&self, request: &Request) -> Option<Response> {
        if request.method != "GET" {
            return None;
        }
        let (content_type, body) = match request.path.as_str() {
            "/" | "/dashboard" | "/dashboard/" => (
                "text/html; charset=utf-8",
                include_bytes!("../../dashboard/index.html").as_slice(),
            ),
            "/dashboard/app.js" => (
                "text/javascript; charset=utf-8",
                include_bytes!("../../dashboard/app.js").as_slice(),
            ),
            "/dashboard/style.css" => (
                "text/css; charset=utf-8",
                include_bytes!("../../dashboard/style.css").as_slice(),
            ),
            _ => return None,
        };
        Some(Response{status:200,headers:BTreeMap::from([("content-type".into(),content_type.into()),("cache-control".into(),"no-store".into()),("content-security-policy".into(),"default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'".into()),("x-content-type-options".into(),"nosniff".into()),("referrer-policy".into(),"no-referrer".into())]),body:body.to_vec()})
    }
    pub(crate) async fn admin_identity(&self, request: &Request) -> Result<Option<String>> {
        match self.admin_auth(request) {
            Ok(()) => return Ok(None),
            Err(e) if e.code != "UNAUTHENTICATED" => return Err(e),
            _ => {}
        }
        let credential = bearer(request)?;
        let rows=self.db.query(Statement::new("SELECT space_id FROM space_admin_credentials WHERE credential_digest=? AND revoked_at IS NULL",vec![Sha256::digest(credential.as_bytes()).to_vec().into()])).await?;
        Ok(Some(text(
            rows.first().ok_or_else(Error::unauthenticated)?,
            "space_id",
        )?))
    }
    pub(crate) async fn admin_access(&self, request: &Request) -> Result<Option<String>> {
        if request.headers.contains_key("authorization") {
            return self.admin_identity(request).await;
        }
        let cookie = session_cookie(request)?;
        let scope = self.valid_session(&cookie).await?;
        if request.method != "GET" {
            let csrf = self.csrf_token(&cookie)?;
            let supplied = request
                .headers
                .get("x-waypost-csrf")
                .ok_or_else(Error::forbidden)?;
            if Sha256::digest(csrf.as_bytes()) != Sha256::digest(supplied.as_bytes()) {
                return Err(Error::forbidden());
            }
        }
        Ok(scope)
    }
    fn csrf_token(&self, cookie: &str) -> Result<String> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.csrf_key).map_err(|_| Error::storage())?;
        mac.update(b"waypost-admin-csrf\0");
        mac.update(cookie.as_bytes());
        Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }
    async fn valid_session(&self, cookie: &str) -> Result<Option<String>> {
        let rows=self.db.query(Statement::new("SELECT s.space_id FROM admin_sessions s WHERE s.session_digest=? AND s.expires_at>? AND ((s.space_id IS NULL AND s.admin_fingerprint=?) OR EXISTS(SELECT 1 FROM space_admin_credentials c WHERE c.space_id=s.space_id AND c.credential_digest=s.admin_fingerprint AND c.revoked_at IS NULL))",vec![Sha256::digest(cookie.as_bytes()).to_vec().into(),self.clock.now_ms().into(),Sha256::digest(self.config.admin_secret.as_bytes()).to_vec().into()])).await?;
        let row = rows.first().ok_or_else(Error::unauthenticated)?;
        match row.get("space_id") {
            Some(Sql::Null) => Ok(None),
            Some(Sql::Text(space)) => {
                check_id(space)?;
                Ok(Some(space.clone()))
            }
            _ => Err(Error::internal("admin session scope decoding")),
        }
    }
    pub(crate) async fn admin_session(&self, request: &Request) -> Response {
        let result = self.session_action(request).await;
        match result {
            Ok((data, cookie)) => {
                let mut r = Response::json(200, json!({"ok":true,"data":data}));
                r.headers.insert("cache-control".into(), "no-store".into());
                if let Some(cookie) = cookie {
                    r.headers.insert("set-cookie".into(), cookie);
                }
                r
            }
            Err(e) => Response::json(e.status(), json!({"ok":false,"error":e.value()})),
        }
    }
    async fn session_action(&self, request: &Request) -> Result<(Value, Option<String>)> {
        if request
            .headers
            .get("origin")
            .is_some_and(|o| o != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        let secure = if self.config.public_origin.starts_with("https://") {
            "; Secure"
        } else {
            ""
        };
        if request.method == "POST" {
            let body = crate::json::parse(&request.body, self.limits.max_json_bytes)?;
            fields(&body, &["admin_secret"])?;
            let secret = required(&body, "admin_secret")?;
            let scope = self
                .admin_identity(&Request::json("POST", "/admin/session", &secret, json!({})))
                .await?;
            let cookie = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
            let now = self.clock.now_ms();
            let expires = now + 8 * 60 * 60 * 1000;
            self.db.batch(vec![Statement::new("DELETE FROM admin_sessions WHERE expires_at<=?",vec![now.into()]),Statement::new("INSERT INTO admin_sessions(session_digest,admin_fingerprint,created_at,expires_at,space_id) VALUES(?,?,?,?,?)",vec![Sha256::digest(cookie.as_bytes()).to_vec().into(),Sha256::digest(secret.as_bytes()).to_vec().into(),now.into(),expires.into(),scope.clone().into()])]).await?;
            return Ok((
                json!({"authenticated":true,"csrf_token":self.csrf_token(&cookie)?,"expires_at":stamp(expires)?,"space_id":scope,"operator":scope.is_none()}),
                Some(format!(
                    "waypost_admin={cookie}; Path=/admin; HttpOnly; SameSite=Strict; Max-Age=28800{secure}"
                )),
            ));
        }
        let cookie = session_cookie(request)?;
        let scope = self.admin_access(request).await?;
        if request.method == "GET" {
            return Ok((
                json!({"authenticated":true,"csrf_token":self.csrf_token(&cookie)?,"space_id":scope,"operator":scope.is_none()}),
                None,
            ));
        }
        if request.method == "DELETE" {
            self.db
                .batch(vec![Statement::new(
                    "DELETE FROM admin_sessions WHERE session_digest=?",
                    vec![Sha256::digest(cookie.as_bytes()).to_vec().into()],
                )])
                .await?;
            return Ok((
                json!({"authenticated":false}),
                Some(format!(
                    "waypost_admin=; Path=/admin; HttpOnly; SameSite=Strict; Max-Age=0{secure}"
                )),
            ));
        }
        Err(Error::not_found())
    }
    pub(crate) async fn admin_registry(&self, body: Value) -> Result<Value> {
        fields(&body, &["kind", "space_id", "cursor", "limit"])?;
        let kind = required(&body, "kind")?;
        let (table, id) = match kind.as_str() {
            "spaces" => ("spaces", "space_id"),
            "actors" => ("actors", "actor_id"),
            "consumers" => ("consumers", "consumer_id"),
            _ => return Err(Error::invalid("Unknown registry kind.")),
        };
        let space = if kind == "spaces" {
            None
        } else {
            let sid = required(&body, "space_id")?;
            self.admin_space(&sid).await?;
            Some(sid)
        };
        let scope = fingerprint(&json!({"kind":kind,"space_id":space}))?;
        let mut sql = format!("SELECT * FROM {table} WHERE 1=1");
        let mut args = vec![];
        if let Some(sid) = &space {
            sql.push_str(" AND space_id=?");
            args.push(sid.clone().into());
        }
        if let Some(cursor) = body.get("cursor").filter(|v| !v.is_null()) {
            let payload = self.verify_cursor(
                cursor
                    .as_str()
                    .ok_or_else(|| Error::new("INVALID_CURSOR", "Invalid registry cursor."))?,
            )?;
            let values = payload.split('|').collect::<Vec<_>>();
            if values.len() != 4 || values[0] != "ar1" || values[1] != scope {
                return Err(Error::new(
                    "INVALID_CURSOR",
                    "Registry cursor belongs to another query.",
                ));
            }
            let time = values[2]
                .parse::<i64>()
                .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor time."))?;
            check_id(values[3])?;
            sql.push_str(&format!(" AND (created_at>? OR (created_at=? AND {id}>?))"));
            args.extend(vec![time.into(), time.into(), values[3].into()]);
        }
        let limit = super::mailbox::page_limit(&body, self.limits.max_page_size)?;
        sql.push_str(&format!(" ORDER BY created_at,{id} LIMIT ?"));
        args.push(((limit + 1) as i64).into());
        let mut rows = self.db.query(Statement::new(sql, args)).await?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let cursor = rows
            .last()
            .map(|r| {
                self.sign_cursor(&format!(
                    "ar1|{scope}|{}|{}",
                    integer(r, "created_at")?,
                    text(r, id)?
                ))
            })
            .transpose()?;
        Ok(
            json!({"items":rows.iter().map(row_value).collect::<Vec<_>>(),"has_more":more,"next_cursor":cursor}),
        )
    }
    pub(crate) async fn admin_inspect(&self, sid: &str, body: Value) -> Result<Value> {
        fields(
            &body,
            &[
                "kind",
                "actor_id",
                "resource_id",
                "cursor",
                "limit",
                "revision",
            ],
        )?;
        let kind = required(&body, "kind")?;
        if kind == "message" {
            let id = required(&body, "resource_id")?;
            check_id(&id)?;
            let rows = self
                .db
                .query(Statement::new(
                    "SELECT * FROM messages WHERE space_id=? AND message_id=?",
                    vec![sid.into(), id.clone().into()],
                ))
                .await?;
            let r = rows.first().ok_or_else(Error::not_found)?;
            let mut value = crate::json::parse(
                &self.object_read(sid, &text(r, "body_ref")?).await?,
                self.limits.max_json_bytes,
            )?;
            value["message_id"] = json!(id);
            value["producer_id"] = json!(text(r, "producer_id")?);
            value["thread_id"] = json!(text(r, "thread_id")?);
            value["committed_at"] = json!(stamp(integer(r, "committed_at")?)?);
            return Ok(value);
        }
        if kind == "memory" {
            let cid = required(&body, "resource_id")?;
            let c = self.admin_collection(sid, &cid).await?;
            let revision = match body.get("revision") {
                None | Some(Value::Null) => {
                    c["current_revision"].as_i64().ok_or_else(Error::storage)?
                }
                Some(v) => v
                    .as_i64()
                    .filter(|r| *r >= 0)
                    .ok_or_else(|| Error::invalid("Invalid revision."))?,
            };
            if revision>0 && self.db.query(Statement::new("SELECT 1 AS exists_revision FROM memory_revisions WHERE space_id=? AND collection_id=? AND revision=?",vec![sid.into(),cid.clone().into(),revision.into()])).await?.is_empty(){return Err(Error::not_found())}
            let rows=self.db.query(Statement::new("SELECT item_id,blob_id,path,metadata,deleted_in_revision FROM memory_revision_items WHERE space_id=? AND collection_id=? AND revision=? ORDER BY item_id LIMIT 101",vec![sid.into(),cid.clone().into(),revision.into()])).await?;
            let mut items = vec![];
            for row in rows.iter().take(100) {
                let mut value = row_value(row);
                value["metadata"] = crate::json::parse(
                    self.open_field(
                        sid,
                        &format!("memory/{cid}/{revision}/{}/metadata", text(row, "item_id")?),
                        &text(row, "metadata")?,
                    )?
                    .as_bytes(),
                    self.limits.max_blob_bytes as usize,
                )?;
                value["path"] = json!(self.open_field(
                    sid,
                    &format!("memory/{cid}/{revision}/{}/path", text(row, "item_id")?),
                    &text(row, "path")?
                )?);
                items.push(value);
            }
            return Ok(
                json!({"collection":c,"revision":revision,"items":items,"has_more":rows.len()>100,"limit":100}),
            );
        }
        if kind == "blob_content" {
            let id = required(&body, "resource_id")?;
            check_id(&id)?;
            let rows = self
                .db
                .query(Statement::new(
                    "SELECT * FROM blobs WHERE space_id=? AND blob_id=?",
                    vec![sid.into(), id.clone().into()],
                ))
                .await?;
            let row = rows.first().ok_or_else(Error::not_found)?;
            let bytes = self.read_blob_bytes(row).await?;
            return Ok(
                json!({"blob_id":id,"content_type":text(row,"content_type")?,"size_bytes":integer(row,"size_bytes")?,"sha256":hex::encode(blob(row,"sha256")?),"text":std::str::from_utf8(&bytes).ok(),"inspection":"UTF-8 text rendering; other binary data is not rendered"}),
            );
        }
        if kind == "memory_revision" {
            let cid = required(&body, "resource_id")?;
            check_id(&cid)?;
            let c = self.admin_collection(sid, &cid).await?;
            let rows=self.db.query(Statement::new("SELECT r.revision,r.parent_revision,r.manifest_blob_id,r.publisher_actor_id,r.committed_at,COUNT(i.item_id) AS total,COUNT(CASE WHEN i.deleted_in_revision IS NULL THEN i.item_id END) AS active FROM memory_revisions r LEFT JOIN memory_revision_items i ON i.space_id=r.space_id AND i.collection_id=r.collection_id AND i.revision=r.revision WHERE r.space_id=? AND r.collection_id=? GROUP BY r.revision ORDER BY r.revision DESC LIMIT 100",vec![sid.into(),cid.into()])).await?;
            return Ok(
                json!({"collection":c,"revisions":rows.iter().map(row_value).collect::<Vec<_>>(),"limit":100}),
            );
        }
        let (table, id, sort, projection, actor_column) = match kind.as_str() {
            "feed" => (
                "feed_items",
                "item_id",
                "space_sequence",
                "item_id,item_type,resource_id,resource_version,producer_id,thread_id,committed_at,CAST(space_sequence AS TEXT) AS visible_seq",
                "producer_id",
            ),
            "messages" | "inbox" => (
                "messages",
                "message_id",
                "committed_at",
                "message_id,producer_id,thread_id,in_reply_to,kind,subject,committed_at",
                "producer_id",
            ),
            "submissions" => (
                "submissions",
                "submission_id",
                "created_at",
                "submission_id,producer_id,operation,request_id,resource_id,state,created_at,committed_at,result_revision,result_changed,error_code,error_current_revision",
                "producer_id",
            ),
            "receipts" => (
                "receipts",
                "receipt_id",
                "updated_at",
                "receipt_id,consumer_id,subject_item_id,stage,version,updated_at",
                "consumer_id",
            ),
            "collections" => (
                "memory_collections",
                "collection_id",
                "created_at",
                "collection_id,source_actor_id,scope,display_name,current_revision,created_at",
                "source_actor_id",
            ),
            "tokens" => (
                "tokens",
                "token_id",
                "created_at",
                "token_id,actor_id,consumer_id,expires_at,revoked_at,created_at,(SELECT json_group_array(scope) FROM token_scopes ts WHERE ts.token_id=tokens.token_id) AS scopes_json",
                "actor_id",
            ),
            "blobs" => (
                "blobs",
                "blob_id",
                "sealed_at",
                "blob_id,owner_actor_id,content_type,size_bytes,filename,sealed_at",
                "owner_actor_id",
            ),
            _ => return Err(Error::invalid("Unknown inspection kind.")),
        };
        let actor = match body.get("actor_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => {
                check_id(s)?;
                Some(s.clone())
            }
            _ => return Err(Error::invalid("Invalid Actor filter.")),
        };
        let scope = fingerprint(&json!({"space":sid,"kind":kind,"actor":actor}))?;
        let mut sql = format!(
            "SELECT {projection},{sort} AS sort_at,{id} AS sort_id FROM {table} WHERE space_id=?"
        );
        let mut args = vec![sid.into()];
        if kind == "inbox" {
            sql.push_str(" AND EXISTS(SELECT 1 FROM message_recipients r JOIN spaces s ON s.space_id=r.space_id AND s.default_actor_id=r.actor_id WHERE r.space_id=messages.space_id AND r.message_id=messages.message_id)");
        }
        if let Some(actor) = &actor {
            if kind == "receipts" {
                sql.push_str(" AND EXISTS(SELECT 1 FROM consumers c WHERE c.space_id=receipts.space_id AND c.consumer_id=receipts.consumer_id AND c.actor_id=?)");
            } else {
                sql.push_str(&format!(" AND {actor_column}=?"));
            }
            args.push(actor.clone().into());
        }
        if let Some(cursor) = body.get("cursor").filter(|v| !v.is_null()) {
            let payload = self.verify_cursor(
                cursor
                    .as_str()
                    .ok_or_else(|| Error::new("INVALID_CURSOR", "Invalid cursor."))?,
            )?;
            let f = payload.split('|').collect::<Vec<_>>();
            if f.len() != 4 || f[0] != "a1" || f[1] != scope {
                return Err(Error::new(
                    "INVALID_CURSOR",
                    "Inspection cursor belongs to another query.",
                ));
            }
            let position = f[2]
                .parse::<i64>()
                .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor position."))?;
            check_id(f[3])?;
            sql.push_str(&format!(" AND ({sort}<? OR ({sort}=? AND {id}<?))"));
            args.extend(vec![position.into(), position.into(), f[3].into()]);
        }
        let limit = super::mailbox::page_limit(&body, self.limits.max_page_size)?;
        sql.push_str(&format!(" ORDER BY {sort} DESC,{id} DESC LIMIT ?"));
        args.push(((limit + 1) as i64).into());
        let mut rows = self.db.query(Statement::new(sql, args)).await?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let cursor = rows
            .last()
            .map(|r| {
                self.sign_cursor(&format!(
                    "a1|{scope}|{}|{}",
                    integer(r, "sort_at")?,
                    text(r, "sort_id")?
                ))
            })
            .transpose()?;
        let items = rows
            .iter()
            .map(|r| {
                let mut v = row_value(r);
                if let Some(o) = v.as_object_mut() {
                    o.remove("sort_at");
                    o.remove("sort_id");
                }
                if kind == "tokens" {
                    v["scopes"] = crate::json::parse(
                        text(r, "scopes_json")?.as_bytes(),
                        self.limits.max_json_bytes,
                    )?;
                    v.as_object_mut()
                        .ok_or_else(Error::storage)?
                        .remove("scopes_json");
                }
                if kind == "messages" || kind == "inbox" {
                    v["subject"] = json!(self.open_field(
                        sid,
                        &format!("message/{}/subject", text(r, "message_id")?),
                        &text(r, "subject")?
                    )?);
                }
                if kind == "blobs"
                    && let Some(name) = crate::storage::driver::optional_text(r, "filename")
                {
                    v["filename"] = json!(self.open_field(
                        sid,
                        &format!("blob/{}/filename", text(r, "blob_id")?),
                        &name
                    )?);
                }
                Ok(v)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"items":items,"has_more":more,"next_cursor":cursor}))
    }
}
fn session_cookie(request: &Request) -> Result<String> {
    request
        .headers
        .get("cookie")
        .and_then(|s| {
            s.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                if key == "waypost_admin"
                    && value.len() <= 128
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    Some(value.into())
                } else {
                    None
                }
            })
        })
        .ok_or_else(Error::unauthenticated)
}
fn row_value(row: &Row) -> Value {
    Value::Object(
        row.iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    match value {
                        Sql::Null => Value::Null,
                        Sql::Text(s) => json!(s),
                        Sql::Integer(n) => json!(n),
                        Sql::Blob(_) => Value::Null,
                    },
                )
            })
            .collect(),
    )
}
