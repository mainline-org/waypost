use super::*;
use crate::{
    domain::{Send, check_id},
    storage::driver::optional_text,
};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleInput {
    kind: String,
    #[serde(default)]
    value: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyInput {
    expected_revision: i64,
    enabled: bool,
    mode: String,
    all_agents: bool,
    #[serde(default)]
    actor_ids: Vec<String>,
    #[serde(default)]
    allow_uninspected_attachments: bool,
    #[serde(default)]
    rules: Vec<RuleInput>,
}
struct Rule {
    id: String,
    kind: String,
    value: Option<String>,
    regex: Option<Regex>,
}
pub(super) struct Policy {
    pub(super) revision: i64,
    enabled: bool,
    mode: String,
    all_agents: bool,
    actors: Vec<String>,
    allow_binary: bool,
    rules: Vec<Rule>,
}
pub(crate) struct PrivacyOutput {
    pub input: Send,
    pub revision: i64,
    pub hits: Vec<Vec<Sql>>,
    pub blobs: Vec<Vec<Sql>>,
}
fn compile(kind: &str, value: Option<&str>) -> Result<Option<Regex>> {
    let pattern = match kind {
        "email" => r"(?i)[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9-]+(?:\.[a-z0-9-]+)+",
        "phone" => r"(?:\+\d{1,3}[ -]?)?\d(?:[ -]?\d){7,14}",
        "identity_number" => r"(?i)\b\d{17}[0-9x]\b",
        "credential" => {
            r"(?:sk-[A-Za-z0-9_-]{12,}|wp_[A-Za-z0-9_-]{24,}|AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{20,}|(?i:Bearer)[ ]+[A-Za-z0-9._~+/-]{16,})"
        }
        "regex" => value.ok_or_else(|| Error::invalid("Regex value required."))?,
        "literal" => {
            return Ok(Some(
                RegexBuilder::new(&regex::escape(
                    value.ok_or_else(|| Error::invalid("Literal value required."))?,
                ))
                .size_limit(1_048_576)
                .build()
                .map_err(|_| Error::invalid("Invalid privacy rule."))?,
            ));
        }
        "metadata_key" => return Ok(None),
        _ => return Err(Error::invalid("Unknown privacy detector.")),
    };
    Ok(Some(
        RegexBuilder::new(pattern)
            .size_limit(1_048_576)
            .build()
            .map_err(|_| Error::invalid("Invalid privacy regex."))?,
    ))
}
impl Policy {
    fn mask(
        &self,
        input: &str,
        field: &str,
        hits: &mut BTreeMap<(String, String), i64>,
        budget: &mut usize,
    ) -> Result<String> {
        if input.len() > *budget {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Privacy output exceeds deployment limit.",
            ));
        }
        let mut output = input.to_owned();
        for rule in &self.rules {
            if let Some(regex) = &rule.regex {
                let mut next = String::new();
                let mut end = 0;
                let mut count = 0;
                for found in regex.find_iter(&output) {
                    let part = &output[end..found.start()];
                    if next.len() + part.len() + 10 > *budget {
                        return Err(Error::new(
                            "RESOURCE_LIMIT",
                            "Privacy output exceeds deployment limit.",
                        ));
                    }
                    next.push_str(part);
                    next.push_str("[REDACTED]");
                    end = found.end();
                    count += 1;
                }
                if count > 0 {
                    if next.len() + output.len() - end > *budget {
                        return Err(Error::new(
                            "RESOURCE_LIMIT",
                            "Privacy output exceeds deployment limit.",
                        ));
                    }
                    next.push_str(&output[end..]);
                    *hits.entry((rule.id.clone(), field.into())).or_default() += count;
                    output = next;
                }
            }
        }
        *budget -= output.len();
        Ok(output)
    }
    fn metadata(
        &self,
        input: &Value,
        hits: &mut BTreeMap<(String, String), i64>,
        budget: &mut usize,
    ) -> Result<Value> {
        Ok(match input {
            Value::String(s) => json!(self.mask(s, "metadata.value", hits, budget)?),
            Value::Number(n) => {
                let text = n.to_string();
                let masked = self.mask(&text, "metadata.value", hits, budget)?;
                if masked != text {
                    json!(masked)
                } else {
                    input.clone()
                }
            }
            Value::Array(a) => Value::Array(
                a.iter()
                    .map(|v| self.metadata(v, hits, budget))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Object(o) => {
                let mut result = serde_json::Map::new();
                for (key, value) in o {
                    let masked = self.mask(key, "metadata.key", hits, budget)?;
                    let private = self.rules.iter().find(|r| {
                        r.kind == "metadata_key"
                            && r.value
                                .as_ref()
                                .is_some_and(|v| v.eq_ignore_ascii_case(key))
                    });
                    let filtered = if let Some(rule) = private {
                        if *budget < 10 {
                            return Err(Error::new(
                                "RESOURCE_LIMIT",
                                "Privacy output exceeds deployment limit.",
                            ));
                        }
                        *budget -= 10;
                        *hits
                            .entry((rule.id.clone(), "metadata.value".into()))
                            .or_default() += 1;
                        json!("[REDACTED]")
                    } else {
                        self.metadata(value, hits, budget)?
                    };
                    let mut unique = masked.clone();
                    let mut suffix = 1;
                    while result.contains_key(&unique) {
                        unique = format!("{masked}_{suffix}");
                        suffix += 1;
                    }
                    result.insert(unique, filtered);
                }
                Value::Object(result)
            }
            _ => input.clone(),
        })
    }
}
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(super) async fn policy(&self, space: &str) -> Result<Policy> {
        let rows=self.db.query(Statement::new("SELECT s.privacy_revision,p.enabled,p.mode,p.all_agents,p.allow_uninspected_attachments FROM spaces s LEFT JOIN privacy_policies p ON p.space_id=s.space_id WHERE s.space_id=?",vec![space.into()])).await?;
        let r = rows.first().ok_or_else(Error::not_found)?;
        let actors = self
            .db
            .query(Statement::new(
                "SELECT actor_id FROM privacy_targets WHERE space_id=? ORDER BY actor_id",
                vec![space.into()],
            ))
            .await?
            .iter()
            .map(|r| text(r, "actor_id"))
            .collect::<Result<Vec<_>>>()?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM privacy_rules WHERE space_id=? ORDER BY position",
                vec![space.into()],
            ))
            .await?;
        let mut rules = vec![];
        for row in rows {
            let kind = text(&row, "kind")?;
            let value = optional_text(&row, "value");
            let regex = compile(&kind, value.as_deref())?;
            rules.push(Rule {
                id: text(&row, "rule_id")?,
                kind,
                value,
                regex,
            })
        }
        Ok(Policy {
            revision: integer(r, "privacy_revision")?,
            enabled: integer(r, "enabled").unwrap_or(0) == 1,
            mode: optional_text(r, "mode").unwrap_or_else(|| "redact".into()),
            all_agents: integer(r, "all_agents").unwrap_or(0) == 1,
            actors,
            allow_binary: integer(r, "allow_uninspected_attachments").unwrap_or(0) == 1,
            rules,
        })
    }
    pub(super) async fn filter_message(
        &self,
        p: &Principal,
        row: &Row,
        mut input: Send,
        policy: Policy,
    ) -> Result<PrivacyOutput> {
        let mut output = PrivacyOutput {
            input: input.clone(),
            revision: policy.revision,
            hits: vec![],
            blobs: vec![],
        };
        let mut actors = input.recipient_ids.clone().unwrap_or_default();
        actors.push(p.actor_id.clone());
        let selected = if policy.all_agents {
            let mut args = vec![p.space_id.clone().into()];
            args.extend(actors.iter().cloned().map(Sql::from));
            !self.db.query(Statement::new(format!("SELECT 1 AS selected FROM actors WHERE space_id=? AND kind='agent' AND actor_id IN ({}) LIMIT 1",vec!["?";actors.len()].join(",")),args)).await?.is_empty()
        } else {
            actors.iter().any(|a| policy.actors.contains(a))
        };
        if !policy.enabled || !selected {
            return Ok(output);
        }
        let mut hits = BTreeMap::new();
        let mut budget = self.limits.max_json_bytes;
        input.subject = policy.mask(&input.subject, "subject", &mut hits, &mut budget)?;
        input.body.text = policy.mask(&input.body.text, "body.text", &mut hits, &mut budget)?;
        for option in &mut input.body.options {
            *option = policy.mask(option, "body.options", &mut hits, &mut budget)?
        }
        if let Some(source) = &mut input.source {
            for v in [
                &mut source.native_thread_id,
                &mut source.native_message_id,
                &mut source.url,
            ]
            .into_iter()
            .flatten()
            {
                *v = policy.mask(v, "source.value", &mut hits, &mut budget)?
            }
        }
        let metadata = policy.metadata(
            &Value::Object(input.metadata.clone()),
            &mut hits,
            &mut budget,
        )?;
        input.metadata = metadata.as_object().cloned().ok_or_else(Error::storage)?;
        let sid = text(row, "submission_id")?;
        let now = self.clock.now_ms();
        for (position, id) in input.attachment_ids.iter_mut().enumerate() {
            let original = self.reference_blob(p, id).await?;
            let original_bytes = self.read_blob_bytes(&original).await?;
            let content_type = text(&original, "content_type")?;
            let is_text = content_type.starts_with("text/")
                || content_type.split(';').next() == Some("application/json")
                || content_type.ends_with("+json");
            let bytes = if is_text {
                let text = std::str::from_utf8(&original_bytes).map_err(|_| {
                    Error::new(
                        "PRIVACY_BLOCKED",
                        "Text attachment cannot be inspected as UTF-8.",
                    )
                })?;
                policy
                    .mask(
                        text,
                        "attachment.content",
                        &mut hits,
                        &mut (self.limits.max_blob_bytes as usize),
                    )?
                    .into_bytes()
            } else {
                if !policy.allow_binary {
                    return Err(Error::new(
                        "PRIVACY_BLOCKED",
                        "This attachment type cannot be inspected by the configured policy.",
                    ));
                }
                *hits
                    .entry(("uninspected_attachment".into(), "attachment.content".into()))
                    .or_default() += 1;
                original_bytes
            };
            let filename = optional_text(&original, "filename")
                .map(|v| {
                    self.open_field(
                        &p.space_id,
                        &format!("blob/{}/filename", text(&original, "blob_id")?),
                        &v,
                    )
                })
                .transpose()?
                .map(|v| policy.mask(&v, "attachment.filename", &mut hits, &mut 256))
                .transpose()?;
            if bytes.len() as u64 > self.limits.max_blob_bytes {
                return Err(Error::new(
                    "RESOURCE_LIMIT",
                    "Filtered attachment exceeds deployment limit.",
                ));
            }
            let derived = format!("filtered_{sid}_p{}_a{position}", policy.revision);
            self.journal_object(
                p,
                &sid,
                &self.storage_key(&p.space_id, &format!("object_{derived}")),
            )
            .await?;
            let object = self
                .object_create(
                    &p.space_id,
                    &self.storage_key(&p.space_id, &format!("object_{derived}")),
                    &bytes,
                    &content_type,
                    &Sha256::digest(&bytes),
                )
                .await?;
            output.blobs.push(vec![
                p.space_id.clone().into(),
                derived.clone().into(),
                p.actor_id.clone().into(),
                self.storage_key(&p.space_id, &format!("object_{derived}"))
                    .into(),
                content_type.into(),
                (object.size as i64).into(),
                object.sha256.into(),
                filename
                    .map(|name| {
                        self.seal_field(&p.space_id, &format!("blob/{derived}/filename"), &name)
                    })
                    .transpose()?
                    .into(),
                now.into(),
            ]);
            *id = derived;
        }
        if policy.mode == "block"
            && hits
                .keys()
                .any(|(rule, _)| rule != "uninspected_attachment")
        {
            return Err(Error::new(
                "PRIVACY_BLOCKED",
                "Configured privacy rules blocked this message.",
            ));
        }
        let action = if hits.is_empty() {
            "passed"
        } else {
            "redacted"
        };
        if hits.is_empty() {
            hits.insert(("policy".into(), "message".into()), 0);
        }
        output.hits = hits
            .into_iter()
            .map(|((rule, field), count)| {
                vec![
                    p.space_id.clone().into(),
                    sid.clone().into(),
                    policy.revision.into(),
                    action.into(),
                    rule.into(),
                    field.into(),
                    count.into(),
                    now.into(),
                ]
            })
            .collect();
        output.input = input;
        Ok(output)
    }
    pub(crate) async fn admin_privacy(
        &self,
        sid: &str,
        request: &Request,
        parts: &[&str],
        body: Value,
    ) -> Result<Value> {
        if parts.len() == 6 && parts[5] == "preview" && request.method == "POST" {
            crate::json::fields(&body, &["text"])?;
            let text = crate::json::required(&body, "text")?;
            let p = self.policy(sid).await?;
            let mut hits = BTreeMap::new();
            let mut budget = self.limits.max_json_bytes;
            let filtered = p.mask(&text, "preview.text", &mut hits, &mut budget)?;
            return Ok(
                json!({"revision":p.revision,"mode":p.mode,"would_block":p.enabled && p.mode=="block" && !hits.is_empty(),"filtered_text":filtered,"hits":hits.into_iter().map(|((rule_id,field),count)|json!({"rule_id":rule_id,"field":field,"count":count})).collect::<Vec<_>>(),"preview_only":true}),
            );
        }
        if parts.len() == 6 && parts[5] == "audit" && request.method == "GET" {
            let rows=self.db.query(Statement::new("SELECT a.*,s.resource_id FROM privacy_audit a JOIN submissions s ON s.submission_id=a.submission_id WHERE a.space_id=? ORDER BY a.created_at DESC,a.submission_id,a.rule_id,a.field LIMIT 100",vec![sid.into()])).await?;
            let mut audit = vec![];
            for r in rows {
                audit.push(json!({"message_id":text(&r,"resource_id")?,"privacy_revision":integer(&r,"privacy_revision")?,"action":text(&r,"action")?,"rule_id":text(&r,"rule_id")?,"field":text(&r,"field")?,"count":integer(&r,"count")?,"created_at":stamp(integer(&r,"created_at")?)?}))
            }
            return Ok(json!({"events":audit,"limit":100}));
        }
        if parts.len() != 5 {
            return Err(Error::not_found());
        }
        if request.method == "GET" {
            let p = self.policy(sid).await?;
            return Ok(
                json!({"revision":p.revision,"enabled":p.enabled,"mode":p.mode,"all_agents":p.all_agents,"actor_ids":p.actors,"allow_uninspected_attachments":p.allow_binary,"rules":p.rules.iter().map(|r|json!({"rule_id":r.id,"kind":r.kind,"value":r.value})).collect::<Vec<_>>(),"supported_detectors":["credential","email","phone","identity_number","metadata_key","literal","regex"],"limitations":"Local rules inspect UTF-8 text only; they cannot identify every private fact or inspect image/PDF/binary contents."}),
            );
        }
        if request.method != "PUT" {
            return Err(Error::not_found());
        }
        let input: PolicyInput = crate::json::decode(body)?;
        if !["redact", "block"].contains(&input.mode.as_str())
            || input.expected_revision < 0
            || input.expected_revision == i64::MAX
        {
            return Err(Error::invalid("Invalid privacy mode or revision."));
        }
        if input.actor_ids.len() > 64 || input.rules.len() > 32 {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Too many privacy targets or rules.",
            ));
        }
        for id in &input.actor_ids {
            check_id(id)?;
            self.admin_actor(sid, id).await?;
        }
        for rule in &input.rules {
            if ["literal", "regex", "metadata_key"].contains(&rule.kind.as_str())
                && rule
                    .value
                    .as_ref()
                    .is_none_or(|v| v.is_empty() || v.len() > 256)
            {
                return Err(Error::invalid("Rule value must contain 1–256 bytes."));
            }
            if !["literal", "regex", "metadata_key"].contains(&rule.kind.as_str())
                && rule.value.is_some()
            {
                return Err(Error::invalid("Built-in detector cannot have a value."));
            }
            compile(&rule.kind, rule.value.as_deref())?;
        }
        let next = input.expected_revision + 1;
        let nonce = random_id("policy")?;
        let guard = (
            "EXISTS(SELECT 1 FROM privacy_updates WHERE space_id=? AND revision=? AND nonce=?)"
                .into(),
            vec![sid.into(), next.into(), nonce.clone().into()],
        );
        let mut q = vec![Statement::new(
            "INSERT INTO privacy_updates(space_id,revision,nonce) SELECT ?,?,? WHERE EXISTS(SELECT 1 FROM spaces WHERE space_id=? AND privacy_revision=?) ON CONFLICT(space_id,revision) DO NOTHING",
            vec![
                sid.into(),
                next.into(),
                nonce.clone().into(),
                sid.into(),
                input.expected_revision.into(),
            ],
        )];
        let mut args = vec![next.into(), sid.into(), input.expected_revision.into()];
        args.extend(guard.1.clone());
        q.push(Statement::new(format!("UPDATE spaces SET privacy_revision=? WHERE space_id=? AND privacy_revision=? AND {}",guard.0),args));
        let mut args = vec![
            sid.into(),
            input.enabled.into(),
            input.mode.into(),
            input.all_agents.into(),
            input.allow_uninspected_attachments.into(),
        ];
        args.extend(guard.1.clone());
        q.push(Statement::new(format!("INSERT INTO privacy_policies(space_id,enabled,mode,all_agents,allow_uninspected_attachments) SELECT ?,?,?,?,? WHERE {} ON CONFLICT(space_id) DO UPDATE SET enabled=excluded.enabled,mode=excluded.mode,all_agents=excluded.all_agents,allow_uninspected_attachments=excluded.allow_uninspected_attachments",guard.0),args));
        for table in ["privacy_targets", "privacy_rules"] {
            let mut args = vec![sid.into()];
            args.extend(guard.1.clone());
            q.push(Statement::new(
                format!("DELETE FROM {table} WHERE space_id=? AND {}", guard.0),
                args,
            ));
        }
        q.extend(push_rows(
            "privacy_targets",
            "space_id,actor_id",
            input
                .actor_ids
                .into_iter()
                .map(|id| vec![sid.into(), id.into()])
                .collect(),
            Some(guard.clone()),
        ));
        let mut rules = vec![];
        for (position, rule) in input.rules.into_iter().enumerate() {
            rules.push(vec![
                sid.into(),
                random_id("rule")?.into(),
                (position as i64).into(),
                rule.kind.into(),
                rule.value.into(),
            ]);
        }
        q.extend(push_rows(
            "privacy_rules",
            "space_id,rule_id,position,kind,value",
            rules,
            Some(guard.clone()),
        ));
        self.db.batch(q).await?;
        let rows = self
            .db
            .query(Statement::new(
                format!("SELECT 1 AS applied WHERE {}", guard.0),
                guard.1,
            ))
            .await?;
        if rows.is_empty() {
            return Err(Error::new(
                "REVISION_CONFLICT",
                "Privacy policy changed; read its current revision.",
            ));
        }
        Ok(json!({"revision":next,"updated":true}))
    }
}
