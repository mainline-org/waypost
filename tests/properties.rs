mod common;
use common::*;
use proptest::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use waypost::Request;
async fn empty_blob(f: &Fixture) -> Value {
    let p=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":"property-blob","content_type":"text/plain","size_bytes":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"})).await;
    let mut r = Request::json(
        "PUT",
        &format!("/v0/uploads/{}/content", p["upload_id"].as_str().unwrap()),
        &f.writer_token,
        json!({}),
    );
    r.body = vec![];
    r.headers.insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(r).await.status, 200);
    call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{}/commit", p["upload_id"].as_str().unwrap()),
        &f.writer_token,
        json!({"request_id":"property-seal"}),
    )
    .await["blob_id"]
        .clone()
}
proptest! {
 #![proptest_config(ProptestConfig::with_cases(12))]
 #[test]
 fn memory_operation_sequences_preserve_source_state_history_and_tombstones(operations in prop::collection::vec((0u8..3,0u8..5),1..16)){
  tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
   let f=Fixture::new().await;let blob=empty_blob(&f).await;
   let c=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"property-collection","source_actor_id":f.writer["actor_id"],"scope":"property","display_name":"Property"})).await;
   let cid=c["collection_id"].as_str().unwrap();let path=format!("/v0/memory/collections/{cid}/revisions");let read=format!("/v0/memory/collections/{cid}/read");
   let mut active=BTreeSet::new();let mut deleted=BTreeMap::new();let mut revision=0u64;let mut history=vec![(0,active.clone())];
   let mut first_request=None;
   for (position,(action,id)) in operations.iter().enumerate(){
    let before=active.clone();let mut desired=active.clone();
    let item=|id:u8|json!({"item_id":format!("item{id}"),"blob_id":blob,"path":format!("items/{id}.md")});
    let mut input=json!({"request_id":format!("operation-{position}"),"expected_revision":revision,"mode":"delta","items":[],"delete_ids":[]});
    match action{
     0=>{desired.insert(*id);input["items"]=json!([item(*id)]);},
     1=>{desired.remove(id);input["delete_ids"]=json!([format!("item{id}")]);},
     _=>{desired=(0..*id).collect();input["mode"]=json!("snapshot");input["items"]=json!((0..*id).map(item).collect::<Vec<_>>());},
    }
    let changed=before!=desired;if changed{revision+=1;}
    for removed in before.difference(&desired){deleted.insert(*removed,revision);}
    for id in &desired{deleted.remove(id);}active=desired;
    let published=call(&f.app,"POST",&path,&f.writer_token,input.clone()).await;
    assert_eq!(published["revision"],revision);assert_eq!(published["changed"],changed);
    assert_eq!(published,call(&f.app,"POST",&path,&f.writer_token,input.clone()).await);
    if first_request.is_none(){first_request=Some((input,published));}
    let state=call(&f.app,"POST",&read,&f.writer_token,json!({"include_tombstones":true})).await;
    assert_eq!(state["active_items"],active.len());assert_eq!(state["tombstones"],deleted.len());
    let observed=state["items"].as_array().unwrap().iter().filter(|i|i.get("deleted_in_revision").is_none()).map(|i|i["item_id"].as_str().unwrap().to_owned()).collect::<BTreeSet<_>>();
    assert_eq!(observed,active.iter().map(|id|format!("item{id}")).collect());
    for (id,deleted_at) in &deleted{let tomb=state["items"].as_array().unwrap().iter().find(|i|i["item_id"]==format!("item{id}")).unwrap();assert_eq!(tomb["deleted_in_revision"],*deleted_at);}
    history.push((revision,active.clone()));
   }
   let (request,result)=first_request.unwrap();assert_eq!(result,call(&f.app,"POST",&path,&f.writer_token,request).await);
   let reopened=open(f.dir.path());let (old_revision,old_active)=&history[history.len()/2];
   let old=call(&reopened,"POST",&read,&f.writer_token,json!({"revision":old_revision})).await;assert_eq!(old["active_items"],old_active.len());
   let feed=f.writer_inbox().await;assert_eq!(feed["items"].as_array().unwrap().len(),revision as usize);
  });
 }
 #[test]
 fn receipt_sequences_are_monotonic_independent_and_do_not_amplify_duplicates(stages in prop::collection::vec(any::<bool>(),1..20)){
  tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
   let f=Fixture::new().await;f.send("receipt-subject","Reliable input").await;let page=f.inbox().await;let item=page["items"][0]["item_id"].clone();let mut current=0;let mut transitions=0;
   for (position,processed) in stages.iter().enumerate(){let desired=if *processed{2}else{1};if desired>current{current=desired;transitions+=1;}
    let body=json!({"request_id":format!("receipt-{position}"),"consumer_id":f.space["default_consumer_id"],"items":[{"item_id":item,"stage":if *processed{"processed"}else{"received"}}]});
    let result=call(&f.app,"POST","/v0/receipts",&f.reader_token,body.clone()).await;assert_eq!(result["items"][0]["stage"],if current==2{"processed"}else{"received"});assert_eq!(result,call(&f.app,"POST","/v0/receipts",&f.reader_token,body).await);
   }
   assert_eq!(f.writer_inbox().await["items"].as_array().unwrap().len(),transitions);
   let c=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/consumers",f.sid()),ADMIN,json!({"request_id":"second-consumer","actor_id":f.space["default_actor_id"],"view":"inbox","filter":{}})).await;
   let t=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),ADMIN,json!({"request_id":"second-token","actor_id":f.space["default_actor_id"],"consumer_id":c["consumer_id"],"scopes":["message.read","receipt.write"],"resource_grants":{}})).await;
   let token=t["token"].as_str().unwrap();let result=call(&f.app,"POST","/v0/receipts",token,json!({"request_id":"independent-receipt","consumer_id":c["consumer_id"],"items":[{"item_id":item,"stage":"received"}]})).await;assert_eq!(result["items"][0]["stage"],"received");assert_eq!(result["items"][0]["version"],1);
   assert_eq!(raw(&f.app,"POST","/v0/inbox/read",token,json!({"consumer_id":c["consumer_id"],"cursor":page["next_cursor"]})).await.0,409);
  });
 }
}
