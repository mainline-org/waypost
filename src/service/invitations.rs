use super::admin::{admin_guard, admin_key, admin_reservation};
use super::*;
use crate::{
    domain::check_id,
    json::{fields, required},
};

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn space_invitation(
        &self,
        sid: &str,
        request: &Request,
        parts: &[&str],
        body: Value,
    ) -> Result<Value> {
        if request.method == "DELETE" && parts.len() == 6 {
            check_id(parts[5])?;
            self.db.batch(vec![Statement::new("UPDATE space_invitations SET state='revoked' WHERE space_id=? AND invitation_id=? AND state='pending'",vec![sid.into(),parts[5].into()])]).await?;
            return Ok(json!({"revoked":true}));
        }
        if request.method != "POST" || parts.len() != 5 {
            return Err(Error::not_found());
        }
        fields(&body, &["request_id", "ttl_seconds", "access_request_id"])?;
        let ttl = body
            .get("ttl_seconds")
            .map(|v| {
                v.as_i64()
                    .filter(|n| *n >= 60 && *n <= 604800)
                    .ok_or_else(|| {
                        Error::invalid("Invitation lifetime must be between 60 seconds and 7 days.")
                    })
            })
            .transpose()?
            .unwrap_or(86400);
        let access_id = match body.get("access_request_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) => {
                crate::domain::check_request_id(id)?;
                let rows = self
                    .db
                    .query(Statement::new(
                        "SELECT space_id FROM access_requests WHERE request_id=?",
                        vec![id.clone().into()],
                    ))
                    .await?;
                let row = rows.first().ok_or_else(Error::not_found)?;
                if crate::storage::driver::optional_text(row, "space_id")
                    .is_some_and(|assigned| assigned != sid)
                {
                    return Err(Error::new(
                        "REVISION_CONFLICT",
                        "Request already belongs to another space.",
                    ));
                }
                Some(id.clone())
            }
            _ => return Err(Error::invalid("Invalid access request.")),
        };
        let op = format!("admin.invitation.create:{sid}");
        let (key, fp) = admin_key(&body)?;
        if let Some(row) = self.admin_existing(&op, &key, &fp).await? {
            let id = text(&row, "resource_id")?;
            if self
                .db
                .query(Statement::new(
                    "SELECT 1 AS valid FROM space_invitations WHERE space_id=? AND invitation_id=?",
                    vec![sid.into(), id.into()],
                ))
                .await?
                .is_empty()
            {
                return Err(Error::new(
                    "REVISION_CONFLICT",
                    "Invitation was not issued; use a new request.",
                ));
            }
            return Ok(
                json!({"invitation_id":text(&row,"resource_id")?,"invite_token":null,"token_available":false}),
            );
        }
        let credentials = self
            .db
            .query(Statement::new(
                "SELECT version,revoked_at FROM space_admin_credentials WHERE space_id=?",
                vec![sid.into()],
            ))
            .await?;
        if credentials
            .first()
            .is_some_and(|r| matches!(r.get("revoked_at"), Some(Sql::Null)))
        {
            return Err(Error::new(
                "REVISION_CONFLICT",
                "Space already has an active owner credential.",
            ));
        }
        let version = credentials
            .first()
            .map(|r| integer(r, "version"))
            .transpose()?
            .unwrap_or(0);
        let iid = random_id("invite")?;
        let raw = format!("wpi_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()?));
        let digest = Sha256::digest(raw.as_bytes()).to_vec();
        let now = self.clock.now_ms();
        let expires = now + ttl * 1000;
        let mut guard = admin_guard(&op, &key, &iid);
        let mut statements = vec![admin_reservation(
            &op,
            &key,
            &fp,
            Some(sid.into()),
            &iid,
            None,
            now,
        )?];
        if let Some(access_id) = &access_id {
            let mut args = vec![
                sid.into(),
                iid.clone().into(),
                access_id.clone().into(),
                sid.into(),
            ];
            args.extend(guard.1.clone());
            args.extend(vec![sid.into(), sid.into(), version.into()]);
            statements.push(Statement::new(format!("UPDATE access_requests SET space_id=?,approval_nonce=? WHERE request_id=? AND (space_id IS NULL OR space_id=?) AND {} AND NOT EXISTS(SELECT 1 FROM space_admin_credentials c WHERE c.space_id=? AND c.revoked_at IS NULL) AND COALESCE((SELECT version FROM space_admin_credentials c WHERE c.space_id=?),0)=?",guard.0),args));
            guard.0.push_str(" AND EXISTS(SELECT 1 FROM access_requests WHERE request_id=? AND space_id=? AND approval_nonce=?)");
            guard.1.extend(vec![
                access_id.clone().into(),
                sid.into(),
                iid.clone().into(),
            ]);
        }
        let mut args = vec![sid.into()];
        args.extend(guard.1.clone());
        statements.push(Statement::new(format!("UPDATE space_invitations SET state='revoked' WHERE space_id=? AND state='pending' AND {}",guard.0),args));
        let mut args = vec![
            iid.clone().into(),
            sid.into(),
            digest.clone().into(),
            version.into(),
            now.into(),
            expires.into(),
        ];
        args.extend(guard.1);
        args.extend(vec![sid.into(), sid.into(), version.into()]);
        statements.push(Statement::new(format!("INSERT INTO space_invitations(invitation_id,space_id,token_digest,owner_version,state,created_at,expires_at) SELECT ?,?,?,?,'pending',?,? WHERE {} AND NOT EXISTS(SELECT 1 FROM space_admin_credentials c WHERE c.space_id=? AND c.revoked_at IS NULL) AND COALESCE((SELECT version FROM space_admin_credentials c WHERE c.space_id=?),0)=?",guard.0),args));
        if let Some(access_id) = access_id {
            statements.push(Statement::new("UPDATE access_requests SET status='issued' WHERE request_id=? AND space_id=? AND approval_nonce=? AND EXISTS(SELECT 1 FROM space_invitations WHERE invitation_id=? AND state='pending')",vec![access_id.into(),sid.into(),iid.clone().into(),iid.clone().into()]));
        }
        self.db.batch(statements).await?;
        let own=self.db.query(Statement::new("SELECT invitation_id FROM space_invitations WHERE invitation_id=? AND token_digest=? AND state='pending'",vec![iid.clone().into(),digest.into()])).await?;
        if own.is_empty() {
            return Err(Error::new(
                "REVISION_CONFLICT",
                "Invitation changed; issue a new request.",
            ));
        }
        Ok(
            json!({"invitation_id":iid,"space_id":sid,"invite_token":raw,"token_available":true,"expires_at":stamp(expires)?,"invite_url":format!("{}/dashboard#invite={raw}",self.config.public_origin)}),
        )
    }

    pub(crate) async fn redeem_invitation(&self, request: &Request) -> Response {
        match self.redeem_invitation_data(request).await {
            Ok((data, cookie)) => {
                let mut response = Response::json(200, json!({"ok":true,"data":data}));
                response.headers.insert("set-cookie".into(), cookie);
                response
            }
            Err(e) => Response::json(e.status(), json!({"ok":false,"error":e.value()})),
        }
    }

    async fn redeem_invitation_data(&self, request: &Request) -> Result<(Value, String)> {
        if request.method != "POST" {
            return Err(Error::not_found());
        }
        if request
            .headers
            .get("origin")
            .is_some_and(|o| o != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        let body = crate::json::parse(&request.body, self.limits.max_json_bytes)?;
        fields(&body, &["invite_token"])?;
        let raw = required(&body, "invite_token")?;
        if !raw.starts_with("wpi_") || raw.len() > 128 {
            return Err(Error::unauthenticated());
        }
        let digest = Sha256::digest(raw.as_bytes()).to_vec();
        let now = self.clock.now_ms();
        let nonce = random_id("redeem")?;
        let credential = format!("wp_user_{}", URL_SAFE_NO_PAD.encode(random_bytes::<32>()?));
        let owner_digest = Sha256::digest(credential.as_bytes()).to_vec();
        let cookie = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
        let expires = now + 8 * 60 * 60 * 1000;
        let guard = "EXISTS(SELECT 1 FROM space_invitations i WHERE i.space_id=? AND i.state='redeemed' AND i.redeem_nonce=?)";
        let rows=self.db.query(Statement::new("SELECT space_id FROM space_invitations WHERE token_digest=? AND state='pending' AND expires_at>?",vec![digest.clone().into(),now.into()])).await?;
        let sid = text(rows.first().ok_or_else(Error::unauthenticated)?, "space_id")?;
        self.db.batch(vec![
            Statement::new("UPDATE space_invitations SET state='redeemed',redeem_nonce=?,redeemed_at=? WHERE token_digest=? AND state='pending' AND expires_at>? AND NOT EXISTS(SELECT 1 FROM space_admin_credentials c WHERE c.space_id=space_invitations.space_id AND c.revoked_at IS NULL) AND COALESCE((SELECT version FROM space_admin_credentials c WHERE c.space_id=space_invitations.space_id),0)=owner_version",vec![nonce.clone().into(),now.into(),digest.into(),now.into()]),
            Statement::new("INSERT INTO space_admin_credentials(space_id,credential_digest,version,created_at) SELECT space_id,?,owner_version+1,? FROM space_invitations WHERE state='redeemed' AND redeem_nonce=? ON CONFLICT(space_id) DO UPDATE SET credential_digest=excluded.credential_digest,version=excluded.version,revoked_at=NULL,created_at=excluded.created_at WHERE space_admin_credentials.version=excluded.version-1 AND space_admin_credentials.revoked_at IS NOT NULL",vec![owner_digest.clone().into(),now.into(),nonce.clone().into()]),
            Statement::new(format!("INSERT INTO admin_sessions(session_digest,admin_fingerprint,created_at,expires_at,space_id) SELECT ?,?,?,?,? WHERE {guard} AND EXISTS(SELECT 1 FROM space_admin_credentials c WHERE c.space_id=? AND c.credential_digest=? AND c.revoked_at IS NULL)"),vec![Sha256::digest(cookie.as_bytes()).to_vec().into(),owner_digest.clone().into(),now.into(),expires.into(),sid.clone().into(),sid.clone().into(),nonce.clone().into(),sid.clone().into(),owner_digest.into()]),
        ]).await?;
        let committed=self.db.query(Statement::new("SELECT 1 AS valid FROM space_invitations i JOIN admin_sessions s ON s.space_id=i.space_id WHERE i.space_id=? AND i.redeem_nonce=? AND s.session_digest=?",vec![sid.clone().into(),nonce.into(),Sha256::digest(cookie.as_bytes()).to_vec().into()])).await?;
        if committed.is_empty() {
            return Err(Error::unauthenticated());
        }
        let secure = if self.config.public_origin.starts_with("https://") {
            "; Secure"
        } else {
            ""
        };
        Ok((
            json!({"authenticated":true,"space_id":sid,"operator":false,"credential":credential,"csrf_token":self.csrf_token(&cookie)?,"expires_at":stamp(expires)?,"limits":self.limits}),
            format!(
                "waypost_admin={cookie}; Path=/admin; HttpOnly; SameSite=Strict; Max-Age=28800{secure}"
            ),
        ))
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use crate::native::{FileStore, SqliteDb};
    use std::cell::Cell;

    // Hold both public requests after their pre-read, before either can commit.
    struct ConcurrentReads {
        db: SqliteDb,
        arrived: Cell<usize>,
    }
    impl Database for ConcurrentReads {
        async fn query(&self, statement: Statement) -> Result<Vec<Row>> {
            let approval =
                statement.sql == "SELECT space_id FROM access_requests WHERE request_id=?";
            let rows = self.db.query(statement).await?;
            if approval {
                self.arrived.set(self.arrived.get() + 1);
                while self.arrived.get() < 2 {
                    tokio::task::yield_now().await;
                }
            }
            Ok(rows)
        }
        async fn batch(&self, statements: Vec<Statement>) -> Result<Vec<Vec<Row>>> {
            self.db.batch(statements).await
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_approvals_bind_one_request_to_one_space() {
        let dir = tempfile::tempdir().unwrap();
        let admin = "test-only-concurrent-approval-secret";
        let app = App::new(
            ConcurrentReads {
                db: SqliteDb::open(dir.path().join("metadata.sqlite")).unwrap(),
                arrived: Cell::new(0),
            },
            FileStore::open(dir.path().join("objects")).unwrap(),
            Config {
                admin_secret: admin.into(),
                public_origin: "https://example.com".into(),
            },
        );
        let mut spaces = vec![];
        for id in ["one", "two"] {
            let response = app
                .handle(Request::json(
                    "POST",
                    "/admin/v0/spaces",
                    admin,
                    json!({"request_id":id,"display_name":id}),
                ))
                .await;
            assert_eq!(response.status, 200);
            let value: Value = serde_json::from_slice(&response.body).unwrap();
            spaces.push(value["data"]["space_id"].as_str().unwrap().to_owned());
        }
        assert_eq!(
            app.handle(Request::json(
                "POST",
                "/admin/access-request",
                "",
                json!({"request_id":"race","email":"race@example.com"})
            ))
            .await
            .status,
            200
        );
        let (one, two) = futures_util::join!(
            app.handle(Request::json(
                "POST",
                &format!("/admin/v0/spaces/{}/invitations", spaces[0]),
                admin,
                json!({"request_id":"approve-one","access_request_id":"race"})
            )),
            app.handle(Request::json(
                "POST",
                &format!("/admin/v0/spaces/{}/invitations", spaces[1]),
                admin,
                json!({"request_id":"approve-two","access_request_id":"race"})
            )),
        );
        let mut statuses = [one.status, two.status];
        statuses.sort();
        assert_eq!(statuses, [200, 409]);
        let winner: Value = serde_json::from_slice(if one.status == 200 {
            &one.body
        } else {
            &two.body
        })
        .unwrap();
        let listed = app
            .handle(Request::json(
                "POST",
                "/admin/v0/access-requests",
                admin,
                json!({}),
            ))
            .await;
        let listed: Value = serde_json::from_slice(&listed.body).unwrap();
        assert_eq!(
            listed["data"]["items"][0]["space_id"],
            winner["data"]["space_id"]
        );
        assert_eq!(
            app.handle(Request::json(
                "POST",
                "/admin/invitation",
                "",
                json!({"invite_token":winner["data"]["invite_token"]})
            ))
            .await
            .status,
            200
        );
    }
}

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn access_request(&self, request: &Request) -> Response {
        let result:Result<Value>=async {
            if request.method!="POST" { return Err(Error::not_found()); }
            if request.headers.get("origin").is_some_and(|v|v!=&self.config.public_origin) { return Err(Error::forbidden()); }
            let body=crate::json::parse(&request.body,self.limits.max_json_bytes)?;
            fields(&body,&["request_id","email"])?;
            let id=required(&body,"request_id")?;
            crate::domain::check_request_id(&id)?;
            let email=required(&body,"email")?.trim().to_lowercase();
            let valid=email.split_once('@').is_some_and(|(local,domain)|!local.is_empty()&&!domain.is_empty()&&!domain.contains('@'));
            if !valid||email.len()>254||email.chars().any(char::is_whitespace) { return Err(Error::invalid("Enter a valid email address.")); }
            let contact=self.seal_field("onboarding",&format!("access/{id}/email"),&email)?;
            self.db.batch(vec![Statement::new("INSERT INTO access_requests(request_id,contact,created_at) SELECT ?,?,? WHERE (SELECT COUNT(*) FROM access_requests)<1000 ON CONFLICT(request_id) DO NOTHING",vec![id.clone().into(),contact.into(),self.clock.now_ms().into()])]).await?;
            let rows=self.db.query(Statement::new("SELECT contact FROM access_requests WHERE request_id=?",vec![id.clone().into()])).await?;
            let row=rows.first().ok_or_else(||Error::new("RESOURCE_LIMIT","Access request queue is full."))?;
            if self.open_field("onboarding",&format!("access/{id}/email"),&text(row,"contact")?)?!=email { return Err(Error::new("IDEMPOTENCY_CONFLICT","This request ID was used for a different request.")); }
            Ok(json!({"request_id":id,"accepted":true}))
        }.await;
        match result {
            Ok(data) => Response::json(200, json!({"ok":true,"data":data})),
            Err(e) => Response::json(e.status(), json!({"ok":false,"error":e.value()})),
        }
    }

    pub(crate) async fn access_requests(&self, body: Value) -> Result<Value> {
        fields(&body, &["limit", "cursor"])?;
        let limit = super::mailbox::page_limit(&body, self.limits.max_page_size)?;
        let after = match body.get("cursor") {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(cursor)) => {
                let payload = self.verify_cursor(cursor)?;
                payload
                    .strip_prefix("access1|")
                    .ok_or_else(|| Error::new("INVALID_CURSOR", "Invalid access request cursor."))?
                    .to_owned()
            }
            _ => return Err(Error::invalid("Invalid access request cursor.")),
        };
        let rows=self.db.query(Statement::new("SELECT request_id,contact,created_at,status,space_id FROM access_requests WHERE request_id>? ORDER BY request_id LIMIT ?",vec![after.into(),((limit+1)as i64).into()])).await?;
        let items=rows.iter().take(limit).map(|r|->Result<Value>{
            let id=text(r,"request_id")?;
            Ok(json!({"request_id":id,"email":self.open_field("onboarding",&format!("access/{id}/email"),&text(r,"contact")?)?,"created_at":stamp(integer(r,"created_at")?)?,"status":text(r,"status")?,"space_id":crate::storage::driver::optional_text(r,"space_id")}))
        }).collect::<Result<Vec<_>>>()?;
        let next = rows
            .iter()
            .take(limit)
            .next_back()
            .map(|r| self.sign_cursor(&format!("access1|{}", text(r, "request_id")?)))
            .transpose()?;
        Ok(json!({"items":items,"has_more":rows.len()>limit,"next_cursor":next}))
    }
}
