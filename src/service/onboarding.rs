use super::*;
use crate::{domain::check_id, json::fields, storage::driver::optional_text};

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn primary_assistant(
        &self,
        sid: &str,
        request: &Request,
        body: Value,
    ) -> Result<Value> {
        if request.method == "PUT" {
            fields(&body, &["expected_revision", "actor_id"])?;
            let expected = body
                .get("expected_revision")
                .and_then(Value::as_i64)
                .filter(|v| *v >= 0 && *v < i64::MAX)
                .ok_or_else(|| Error::invalid("Invalid primary assistant revision."))?;
            let actor = match body.get("actor_id") {
                Some(Value::Null) => None,
                Some(Value::String(id)) => {
                    check_id(id)?;
                    let actor = self.admin_actor(sid, id).await?;
                    if actor["kind"] != "agent" || actor["disabled"] == true {
                        return Err(Error::invalid("Choose an enabled assistant."));
                    }
                    Some(id.clone())
                }
                _ => return Err(Error::invalid("Missing assistant identity.")),
            };
            let nonce = random_id("primary")?;
            self.db.batch(vec![Statement::new("UPDATE spaces SET primary_actor_id=?,primary_nonce=?,primary_revision=primary_revision+1 WHERE space_id=? AND primary_revision=? AND (? IS NULL OR EXISTS(SELECT 1 FROM actors a WHERE a.space_id=spaces.space_id AND a.actor_id=? AND a.kind='agent' AND a.disabled=0))", vec![actor.clone().into(),nonce.clone().into(),sid.into(),expected.into(),actor.clone().into(),actor.into()])]).await?;
            let state = self.primary_state(sid, Some(&nonce)).await?;
            if state["revision"] != expected + 1 {
                return Err(Error::new(
                    "REVISION_CONFLICT",
                    "Primary assistant changed; refresh before choosing.",
                ));
            }
            return Ok(state);
        }
        if request.method != "GET" {
            return Err(Error::not_found());
        }
        self.primary_state(sid, None).await
    }

    async fn primary_state(&self, sid: &str, nonce: Option<&str>) -> Result<Value> {
        let rows = self.db.query(Statement::new("SELECT s.primary_revision,a.actor_id,a.display_name,a.disabled FROM spaces s LEFT JOIN actors a ON a.space_id=s.space_id AND a.actor_id=s.primary_actor_id WHERE s.space_id=? AND (? IS NULL OR s.primary_nonce=?)",vec![sid.into(),nonce.map(str::to_owned).into(),nonce.map(str::to_owned).into()])).await?;
        let row = rows.first().ok_or_else(|| {
            Error::new(
                "REVISION_CONFLICT",
                "Primary assistant changed; refresh before choosing.",
            )
        })?;
        let actor = optional_text(row, "actor_id").map(|id| -> Result<Value> {
            Ok(json!({"actor_id":id,"display_name":text(row,"display_name")?,"disabled":integer(row,"disabled")? != 0}))
        }).transpose()?;
        Ok(
            json!({"space_id":sid,"revision":integer(row,"primary_revision")?,"primary_assistant":actor,"limits":self.limits}),
        )
    }
}
