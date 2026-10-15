use super::*;
use crate::{
    domain::{check_id, check_request_id},
    json::{fields, required},
};

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(super) async fn user_admin(
        &self,
        request: &Request,
        body: Value,
        space: &str,
    ) -> Result<Value> {
        if request.path == "/admin/v0/registry" && request.method == "POST" {
            fields(&body, &["kind", "space_id", "cursor", "limit"])?;
            if body.get("kind").and_then(Value::as_str) == Some("spaces") {
                if body.get("cursor").is_some_and(|v| !v.is_null()) {
                    return Err(Error::invalid("User Space listing has no cursor."));
                }
                if body
                    .get("space_id")
                    .is_some_and(|v| v.as_str() != Some(space))
                {
                    return Err(Error::not_found());
                }
                return Ok(
                    json!({"items":[self.admin_space(space).await?],"has_more":false,"next_cursor":null}),
                );
            }
            if body.get("space_id").and_then(Value::as_str) != Some(space) {
                return Err(Error::not_found());
            }
            return self.admin_registry(body).await;
        }
        if request.path == "/admin/v0/spaces" && request.method == "GET" {
            return Ok(json!({"spaces":[self.admin_space(space).await?]}));
        }
        let parts = request.path.split('/').collect::<Vec<_>>();
        if parts.len() < 5
            || parts[..4] != ["", "admin", "v0", "spaces"]
            || parts[4] != space
            || parts.get(5) == Some(&"owner-credential")
            || parts.get(5) == Some(&"invitations")
        {
            return Err(Error::not_found());
        }
        self.admin(request, body).await
    }
    pub(crate) async fn admin(&self, request: &Request, body: Value) -> Result<Value> {
        let parts = request
            .path
            .trim_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        if request.method == "POST" && request.path == "/admin/v0/access-requests" {
            return self.access_requests(body).await;
        }
        if request.method == "POST" && request.path == "/admin/v0/maintenance/encrypt" {
            return self.encrypt_existing(body).await;
        }
        if request.method == "POST" && request.path == "/admin/v0/maintenance/cleanup" {
            return self.admin_cleanup(body).await;
        }
        if request.method == "POST" && parts.as_slice() == ["admin", "v0", "registry"] {
            return self.admin_registry(body).await;
        }
        if request.method == "GET" && parts.as_slice() == ["admin", "v0", "spaces"] {
            let rows = self
                .db
                .query(Statement::new(
                    "SELECT * FROM spaces ORDER BY created_at,space_id LIMIT 100",
                    vec![],
                ))
                .await?;
            return Ok(json!({"spaces":rows.iter().map(space_value).collect::<Result<Vec<_>>>()?}));
        }
        if request.method == "POST" && parts.as_slice() == ["admin", "v0", "spaces"] {
            fields(&body, &["request_id", "display_name"])?;
            let name = display_name(&body)?;
            let operation = "admin.space.create";
            let (key, fp) = admin_key(&body)?;
            if let Some(row) = self.admin_existing(operation, &key, &fp).await? {
                return self.admin_space(&text(&row, "resource_id")?).await;
            }
            let sid = random_id("space")?;
            let aid = random_id("actor")?;
            let cid = random_id("consumer")?;
            let now = self.clock.now_ms();
            let guard=("EXISTS(SELECT 1 FROM admin_submissions WHERE operation=? AND request_id=? AND resource_id=?)".into(),vec![operation.into(),key.clone().into(),sid.clone().into()]);
            let mut statements = vec![admin_reservation(
                operation,
                &key,
                &fp,
                Some(sid.clone()),
                &sid,
                Some(cid.clone()),
                now,
            )?];
            statements.extend(push_rows(
                "spaces",
                "space_id,display_name,source_uri,default_actor_id,default_consumer_id,created_at",
                vec![vec![
                    sid.clone().into(),
                    name.into(),
                    uuid_uri()?.into(),
                    aid.clone().into(),
                    cid.clone().into(),
                    now.into(),
                ]],
                Some(guard.clone()),
            ));
            statements.extend(push_rows(
                "actors",
                "space_id,actor_id,display_name,kind,created_at",
                vec![vec![
                    sid.clone().into(),
                    aid.clone().into(),
                    "Owner".into(),
                    "user".into(),
                    now.into(),
                ]],
                Some(guard.clone()),
            ));
            statements.extend(push_rows(
                "consumers",
                "space_id,consumer_id,actor_id,view,created_at",
                vec![vec![
                    sid.clone().into(),
                    cid.into(),
                    aid.into(),
                    "inbox".into(),
                    now.into(),
                ]],
                Some(guard),
            ));
            self.db.batch(statements).await?;
            let row = self
                .admin_existing(operation, &key, &fp)
                .await?
                .ok_or_else(Error::storage)?;
            return self.admin_space(&text(&row, "resource_id")?).await;
        }
        if parts.len() >= 4 && parts[2] == "spaces" {
            let sid = parts[3];
            check_id(sid)?;
            self.admin_space(sid).await?;
            if request.method == "GET" && parts.len() == 5 && parts[4] == "connection-status" {
                return self.connection_status(sid).await;
            }
            if request.method == "POST" && parts.len() == 7 && parts[4] == "memory" {
                let p = self.dashboard_owner(request, sid, Some(parts[5])).await?;
                return match parts[6] {
                    "read" => self.dashboard_memory_read(&p, parts[5], body).await,
                    "edit" => self.dashboard_memory_edit(&p, parts[5], body).await,
                    _ => Err(Error::not_found()),
                };
            }
            if request.method == "POST"
                && parts.len() == 8
                && parts[4] == "memory"
                && parts[6..] == ["edit", "prepare"]
            {
                let p = self.dashboard_owner(request, sid, Some(parts[5])).await?;
                return self.dashboard_memory_prepare(&p, parts[5], body).await;
            }
            if parts.len() >= 5 && parts[4] == "invitations" {
                return self.space_invitation(sid, request, &parts, body).await;
            }
            if parts.len() == 5 && parts[4] == "primary-assistant" {
                return self.primary_assistant(sid, request, body).await;
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "collections" {
                fields(
                    &body,
                    &["request_id", "source_actor_id", "scope", "display_name"],
                )?;
                let source = required(&body, "source_actor_id")?;
                self.admin_actor(sid, &source).await?;
                let scope = required(&body, "scope")?;
                if scope.trim().is_empty() || scope.len() > 256 {
                    return Err(Error::invalid("Invalid scope."));
                }
                let name = display_name(&body)?;
                let op = format!("admin.collection.create:{sid}");
                let (key, fp) = admin_key(&body)?;
                if let Some(row) = self.admin_existing(&op, &key, &fp).await? {
                    return self
                        .admin_collection(sid, &text(&row, "resource_id")?)
                        .await;
                }
                let cid = random_id("collection")?;
                let now = self.clock.now_ms();
                let mut q = vec![admin_reservation(
                    &op,
                    &key,
                    &fp,
                    Some(sid.into()),
                    &cid,
                    None,
                    now,
                )?];
                q.extend(push_rows(
                    "memory_collections",
                    "space_id,collection_id,source_actor_id,scope,display_name,created_at",
                    vec![vec![
                        sid.into(),
                        cid.clone().into(),
                        source.into(),
                        scope.into(),
                        name.into(),
                        now.into(),
                    ]],
                    Some(admin_guard(&op, &key, &cid)),
                ));
                self.db.batch(q).await?;
                let row = self
                    .admin_existing(&op, &key, &fp)
                    .await?
                    .ok_or_else(Error::storage)?;
                return self
                    .admin_collection(sid, &text(&row, "resource_id")?)
                    .await;
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "owner-credential" {
                fields(&body, &["expected_version"])?;
                let expected = body
                    .get("expected_version")
                    .and_then(Value::as_i64)
                    .filter(|v| *v >= 0 && *v < i64::MAX)
                    .ok_or_else(|| {
                        Error::invalid("expected_version must be a nonnegative integer.")
                    })?;
                let credential =
                    format!("wp_user_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()?));
                let digest = Sha256::digest(credential.as_bytes()).to_vec();
                self.db.batch(vec![Statement::new("INSERT INTO space_admin_credentials(space_id,credential_digest,version,created_at) SELECT ?,?,1,? WHERE ?=0 ON CONFLICT(space_id) DO NOTHING",vec![sid.into(),digest.clone().into(),self.clock.now_ms().into(),expected.into()]),Statement::new("UPDATE space_admin_credentials SET credential_digest=?,version=version+1,revoked_at=NULL,created_at=? WHERE space_id=? AND version=?",vec![digest.clone().into(),self.clock.now_ms().into(),sid.into(),expected.into()])]).await?;
                let rows=self.db.query(Statement::new("SELECT version FROM space_admin_credentials WHERE space_id=? AND credential_digest=?",vec![sid.into(),digest.into()])).await?;
                let row = rows.first().ok_or_else(|| {
                    Error::new(
                        "REVISION_CONFLICT",
                        "Owner credential changed; read its current version.",
                    )
                })?;
                return Ok(
                    json!({"space_id":sid,"credential":credential,"version":integer(row,"version")?,"credential_available":true}),
                );
            }
            if request.method == "GET" && parts.len() == 5 && parts[4] == "owner-credential" {
                let rows = self
                    .db
                    .query(Statement::new(
                        "SELECT version,revoked_at FROM space_admin_credentials WHERE space_id=?",
                        vec![sid.into()],
                    ))
                    .await?;
                return Ok(
                    json!({"space_id":sid,"version":rows.first().map(|r|integer(r,"version")).transpose()?.unwrap_or(0),"active":rows.first().is_some_and(|r|matches!(r.get("revoked_at"),Some(Sql::Null)))}),
                );
            }
            if request.method == "DELETE" && parts.len() == 5 && parts[4] == "owner-credential" {
                fields(&body, &[])?;
                self.db.batch(vec![Statement::new("UPDATE space_admin_credentials SET revoked_at=?,version=version+1 WHERE space_id=? AND revoked_at IS NULL",vec![self.clock.now_ms().into(),sid.into()])]).await?;
                return Ok(json!({"revoked":true}));
            }
            if request.method == "DELETE" && parts.len() == 6 && parts[4] == "tokens" {
                check_id(parts[5])?;
                self.db.batch(vec![Statement::new("UPDATE tokens SET revoked_at=?,authority_version=authority_version+1 WHERE space_id=? AND token_id=? AND revoked_at IS NULL",vec![self.clock.now_ms().into(),sid.into(),parts[5].into()])]).await?;
                return Ok(json!({"revoked":true}));
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "inspect" {
                return self.admin_inspect(sid, body).await;
            }
            if parts.len() >= 5 && parts[4] == "privacy" {
                return self.admin_privacy(sid, request, &parts, body).await;
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "actors" {
                fields(&body, &["request_id", "display_name", "kind"])?;
                let name = display_name(&body)?;
                let kind = required(&body, "kind")?;
                if !["user", "agent", "client"].contains(&kind.as_str()) {
                    return Err(Error::invalid("Invalid actor kind."));
                }
                let operation = format!("admin.actor.create:{sid}");
                let (key, fp) = admin_key(&body)?;
                if let Some(row) = self.admin_existing(&operation, &key, &fp).await? {
                    return self.admin_actor(sid, &text(&row, "resource_id")?).await;
                }
                let aid = random_id("actor")?;
                let now = self.clock.now_ms();
                let guard = admin_guard(&operation, &key, &aid);
                let mut q = vec![admin_reservation(
                    &operation,
                    &key,
                    &fp,
                    Some(sid.into()),
                    &aid,
                    None,
                    now,
                )?];
                q.extend(push_rows(
                    "actors",
                    "space_id,actor_id,display_name,kind,created_at",
                    vec![vec![
                        sid.into(),
                        aid.clone().into(),
                        name.into(),
                        kind.into(),
                        now.into(),
                    ]],
                    Some(guard),
                ));
                self.db.batch(q).await?;
                let row = self
                    .admin_existing(&operation, &key, &fp)
                    .await?
                    .ok_or_else(Error::storage)?;
                return self.admin_actor(sid, &text(&row, "resource_id")?).await;
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "consumers" {
                fields(&body, &["request_id", "actor_id", "view", "filter"])?;
                let actor = required(&body, "actor_id")?;
                self.admin_actor(sid, &actor).await?;
                let view = required(&body, "view")?;
                if !["inbox", "space_log"].contains(&view.as_str()) {
                    return Err(Error::invalid("Invalid consumer view."));
                }
                let mut filter: FeedFilter =
                    crate::json::decode(body.get("filter").cloned().unwrap_or(json!({})))?;
                filter.normalize()?;
                let op = format!("admin.consumer.create:{sid}");
                let (key, fp) = admin_key(&body)?;
                if let Some(row) = self.admin_existing(&op, &key, &fp).await? {
                    return self.admin_consumer(sid, &text(&row, "resource_id")?).await;
                }
                let cid = random_id("consumer")?;
                let now = self.clock.now_ms();
                let guard = admin_guard(&op, &key, &cid);
                let mut q = vec![admin_reservation(
                    &op,
                    &key,
                    &fp,
                    Some(sid.into()),
                    &cid,
                    None,
                    now,
                )?];
                q.extend(push_rows(
                    "consumers",
                    "space_id,consumer_id,actor_id,view,created_at",
                    vec![vec![
                        sid.into(),
                        cid.clone().into(),
                        actor.into(),
                        view.into(),
                        now.into(),
                    ]],
                    Some(guard.clone()),
                ));
                let rows = filter_rows(sid, &cid, &filter);
                q.extend(push_rows(
                    "consumer_filters",
                    "space_id,consumer_id,dimension,value",
                    rows,
                    Some(guard.clone()),
                ));
                self.db.batch(q).await?;
                let row = self
                    .admin_existing(&op, &key, &fp)
                    .await?
                    .ok_or_else(Error::storage)?;
                return self.admin_consumer(sid, &text(&row, "resource_id")?).await;
            }
            if request.method == "POST" && parts.len() == 5 && parts[4] == "tokens" {
                fields(
                    &body,
                    &[
                        "request_id",
                        "actor_id",
                        "consumer_id",
                        "scopes",
                        "resource_grants",
                        "expires_at",
                        "default_recipient_id",
                    ],
                )?;
                let actor = required(&body, "actor_id")?;
                let consumer = required(&body, "consumer_id")?;
                let a = self.admin_actor(sid, &actor).await?;
                if a["disabled"] == true {
                    return Err(Error::forbidden());
                }
                let c = self.admin_consumer(sid, &consumer).await?;
                if c["actor_id"] != actor {
                    return Err(Error::invalid("Consumer belongs to another actor."));
                }
                let mut scopes: Vec<String> = crate::json::decode(
                    body.get("scopes")
                        .cloned()
                        .ok_or_else(|| Error::invalid("Missing scopes."))?,
                )?;
                if scopes.iter().any(|s| {
                    ![
                        "message.send",
                        "message.read",
                        "space.observe",
                        "receipt.write",
                        "memory.write",
                        "memory.read",
                        "blob.write",
                        "blob.reference",
                        "event.subscribe",
                    ]
                    .contains(&s.as_str())
                }) {
                    return Err(Error::invalid("Unknown scope."));
                }
                scopes.sort();
                scopes.dedup();
                let mut grants: ResourceGrants =
                    crate::json::decode(body.get("resource_grants").cloned().unwrap_or(json!({})))?;
                for list in [
                    &mut grants.recipient_ids,
                    &mut grants.memory_read_collection_ids,
                    &mut grants.memory_write_collection_ids,
                    &mut grants.blob_reference_ids,
                ] {
                    for id in list.iter() {
                        check_id(id)?;
                    }
                    list.sort();
                    list.dedup();
                    if list.len() > 64 {
                        return Err(Error::new("RESOURCE_LIMIT", "Too many grants."));
                    }
                }
                for recipient in &grants.recipient_ids {
                    self.admin_actor(sid, recipient).await?;
                }
                for id in grants
                    .memory_read_collection_ids
                    .iter()
                    .chain(grants.memory_write_collection_ids.iter())
                {
                    self.admin_collection(sid, id).await?;
                }
                for id in &grants.blob_reference_ids {
                    if self
                        .db
                        .query(Statement::new(
                            "SELECT 1 AS valid FROM blobs WHERE space_id=? AND blob_id=?",
                            vec![sid.into(), id.clone().into()],
                        ))
                        .await?
                        .is_empty()
                    {
                        return Err(Error::not_found());
                    }
                }
                let default_recipient = match body.get("default_recipient_id") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(id)) => {
                        check_id(id)?;
                        self.admin_actor(sid, id).await?;
                        let allowed = if grants.recipient_ids.is_empty() {
                            vec![
                                self.admin_space(sid).await?["default_actor_id"]
                                    .as_str()
                                    .ok_or_else(Error::storage)?
                                    .to_owned(),
                            ]
                        } else {
                            grants.recipient_ids.clone()
                        };
                        if !allowed.contains(id) {
                            return Err(Error::invalid(
                                "Default recipient must be explicitly authorized.",
                            ));
                        }
                        Some(id.clone())
                    }
                    _ => return Err(Error::invalid("Invalid default recipient.")),
                };
                let now = self.clock.now_ms();
                let expires = match body.get("expires_at") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(v)) => Some(
                        chrono::DateTime::parse_from_rfc3339(v)
                            .map_err(|_| Error::invalid("Invalid expiration."))?
                            .timestamp_millis(),
                    ),
                    _ => return Err(Error::invalid("Expiration must be a timestamp.")),
                };
                if expires.is_some_and(|n| n <= now) {
                    return Err(Error::invalid("Expiration is in the past."));
                }
                let op = format!("admin.token.create:{sid}");
                let (key, fp) = admin_key(&body)?;
                if let Some(row) = self.admin_existing(&op, &key, &fp).await? {
                    return Ok(
                        json!({"token_id":text(&row,"resource_id")?,"token":null,"token_available":false}),
                    );
                }
                let tid = random_id("token")?;
                let raw = format!("wp_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()?));
                let digest = Sha256::digest(raw.as_bytes()).to_vec();
                let guard = admin_guard(&op, &key, &tid);
                let mut q = vec![admin_reservation(
                    &op,
                    &key,
                    &fp,
                    Some(sid.into()),
                    &tid,
                    None,
                    now,
                )?];
                q.extend(push_rows(
                    "tokens",
                    "token_id,token_digest,space_id,actor_id,consumer_id,expires_at,created_at,default_recipient_id",
                    vec![vec![
                        tid.clone().into(),
                        digest.into(),
                        sid.into(),
                        actor.into(),
                        consumer.into(),
                        expires.into(),
                        now.into(),
                        default_recipient.into(),
                    ]],
                    Some(guard.clone()),
                ));
                q.extend(push_rows(
                    "token_scopes",
                    "token_id,scope",
                    scopes
                        .into_iter()
                        .map(|s| vec![tid.clone().into(), s.into()])
                        .collect(),
                    Some(guard.clone()),
                ));
                q.extend(push_rows(
                    "token_recipients",
                    "token_id,space_id,actor_id",
                    grants
                        .recipient_ids
                        .into_iter()
                        .map(|id| vec![tid.clone().into(), sid.into(), id.into()])
                        .collect(),
                    Some(guard.clone()),
                ));
                q.extend(push_rows(
                    "token_collections",
                    "token_id,space_id,collection_id,action",
                    grants
                        .memory_read_collection_ids
                        .into_iter()
                        .map(|id| vec![tid.clone().into(), sid.into(), id.into(), "read".into()])
                        .chain(grants.memory_write_collection_ids.into_iter().map(|id| {
                            vec![tid.clone().into(), sid.into(), id.into(), "write".into()]
                        }))
                        .collect(),
                    Some(guard.clone()),
                ));
                q.extend(push_rows(
                    "token_blob_grants",
                    "token_id,space_id,blob_id",
                    grants
                        .blob_reference_ids
                        .into_iter()
                        .map(|id| vec![tid.clone().into(), sid.into(), id.into()])
                        .collect(),
                    Some(guard),
                ));
                self.db.batch(q).await?;
                let row = self
                    .admin_existing(&op, &key, &fp)
                    .await?
                    .ok_or_else(Error::storage)?;
                let winner = text(&row, "resource_id")?;
                return Ok(
                    json!({"token_id":winner,"token":if winner==tid{Some(raw)}else{None},"token_available":winner==tid}),
                );
            }
            if request.method == "POST"
                && parts.len() == 7
                && parts[4] == "actors"
                && parts[6] == "disable"
            {
                fields(&body, &[])?;
                let aid = parts[5];
                self.admin_actor(sid, aid).await?;
                self.db.batch(vec![Statement::new("UPDATE actors SET disabled=1,authority_version=authority_version+1 WHERE space_id=? AND actor_id=? AND disabled=0",vec![sid.into(),aid.into()])]).await?;
                return Ok(json!({"actor_id":aid,"disabled":true}));
            }
            if request.method == "GET" && parts.len() == 5 && parts[4] == "actors" {
                let rows=self.db.query(Statement::new("SELECT * FROM actors WHERE space_id=? ORDER BY created_at,actor_id LIMIT 100",vec![sid.into()])).await?;
                return Ok(
                    json!({"actors":rows.iter().map(actor_value).collect::<Result<Vec<_>>>()?}),
                );
            }
            if request.method == "GET" && parts.len() == 6 && parts[4] == "actors" {
                return self.admin_actor(sid, parts[5]).await;
            }
            if request.method == "GET" && parts.len() == 5 && parts[4] == "consumers" {
                let rows=self.db.query(Statement::new("SELECT * FROM consumers WHERE space_id=? ORDER BY created_at,consumer_id LIMIT 100",vec![sid.into()])).await?;
                return Ok(
                    json!({"consumers":rows.iter().map(consumer_value).collect::<Result<Vec<_>>>()?}),
                );
            }
        }
        if request.method == "DELETE" && parts.len() == 4 && parts[2] == "tokens" {
            check_id(parts[3])?;
            self.db.batch(vec![Statement::new("UPDATE tokens SET revoked_at=?,authority_version=authority_version+1 WHERE token_id=? AND revoked_at IS NULL",vec![self.clock.now_ms().into(),parts[3].into()])]).await?;
            return Ok(json!({"revoked":true}));
        }
        Err(Error::not_found())
    }
    pub(crate) async fn admin_existing(
        &self,
        operation: &str,
        key: &str,
        fp: &str,
    ) -> Result<Option<Row>> {
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM admin_submissions WHERE operation=? AND request_id=?",
                vec![operation.into(), key.into()],
            ))
            .await?;
        if let Some(row) = rows.first() {
            if hex::encode(blob(row, "fingerprint")?) != fp {
                return Err(Error::new(
                    "IDEMPOTENCY_CONFLICT",
                    "Request ID was used with different content.",
                ));
            }
            return Ok(Some(row.clone()));
        }
        Ok(None)
    }
    pub(crate) async fn admin_space(&self, sid: &str) -> Result<Value> {
        check_id(sid)?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM spaces WHERE space_id=?",
                vec![sid.into()],
            ))
            .await?;
        space_value(rows.first().ok_or_else(Error::not_found)?)
    }
    pub(crate) async fn admin_actor(&self, sid: &str, aid: &str) -> Result<Value> {
        check_id(aid)?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM actors WHERE space_id=? AND actor_id=?",
                vec![sid.into(), aid.into()],
            ))
            .await?;
        actor_value(rows.first().ok_or_else(Error::not_found)?)
    }
    pub(crate) async fn admin_consumer(&self, sid: &str, cid: &str) -> Result<Value> {
        check_id(cid)?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM consumers WHERE space_id=? AND consumer_id=?",
                vec![sid.into(), cid.into()],
            ))
            .await?;
        consumer_value(rows.first().ok_or_else(Error::not_found)?)
    }
}
fn display_name(v: &Value) -> Result<String> {
    let n = required(v, "display_name")?;
    if n.trim().is_empty() || n.len() > 256 {
        Err(Error::invalid("Invalid display name."))
    } else {
        Ok(n)
    }
}
pub(crate) fn admin_key(v: &Value) -> Result<(String, String)> {
    let key = match v.get("request_id") {
        Some(Value::String(s)) => s.clone(),
        None => random_id("admin")?,
        _ => return Err(Error::invalid("Invalid request ID.")),
    };
    check_request_id(&key)?;
    Ok((key, fingerprint(v)?))
}
pub(crate) fn admin_guard(op: &str, key: &str, id: &str) -> (String, Vec<Sql>) {
    ("EXISTS(SELECT 1 FROM admin_submissions WHERE operation=? AND request_id=? AND resource_id=?)".into(),vec![op.into(),key.into(),id.into()])
}
pub(crate) fn admin_reservation(
    op: &str,
    key: &str,
    fp: &str,
    sid: Option<String>,
    id: &str,
    sub: Option<String>,
    now: i64,
) -> Result<Statement> {
    Ok(Statement::new(
        "INSERT INTO admin_submissions(operation,request_id,fingerprint,space_id,resource_id,subresource_id,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(operation,request_id) DO NOTHING",
        vec![
            op.into(),
            key.into(),
            hex::decode(fp).map_err(|_| Error::storage())?.into(),
            sid.into(),
            id.into(),
            sub.into(),
            now.into(),
        ],
    ))
}
pub(crate) fn filter_rows(sid: &str, cid: &str, f: &FeedFilter) -> Vec<Vec<Sql>> {
    let mut r = Vec::new();
    for (dim, list) in [
        ("producer_ids", &f.producer_ids),
        ("thread_ids", &f.thread_ids),
        ("item_types", &f.item_types),
        ("collection_ids", &f.collection_ids),
    ] {
        for v in list {
            r.push(vec![sid.into(), cid.into(), dim.into(), v.clone().into()]);
        }
    }
    r
}
pub(crate) fn space_value(r: &Row) -> Result<Value> {
    Ok(
        json!({"space_id":text(r,"space_id")?,"display_name":text(r,"display_name")?,"source_uri":text(r,"source_uri")?,"default_actor_id":text(r,"default_actor_id")?,"default_consumer_id":text(r,"default_consumer_id")?,"created_at":stamp(integer(r,"created_at")?)?}),
    )
}
pub(crate) fn actor_value(r: &Row) -> Result<Value> {
    Ok(
        json!({"space_id":text(r,"space_id")?,"actor_id":text(r,"actor_id")?,"display_name":text(r,"display_name")?,"kind":text(r,"kind")?,"disabled":integer(r,"disabled")?!=0,"created_at":stamp(integer(r,"created_at")?)?}),
    )
}
pub(crate) fn consumer_value(r: &Row) -> Result<Value> {
    Ok(
        json!({"space_id":text(r,"space_id")?,"consumer_id":text(r,"consumer_id")?,"actor_id":text(r,"actor_id")?,"view":text(r,"view")?,"view_revision":integer(r,"view_revision")?,"disabled":integer(r,"disabled")?!=0}),
    )
}
