use super::{FileStore, SqliteDb};
use crate::{
    App, Config, Request, Response,
    error::{Error, Result},
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::{collections::BTreeMap, convert::Infallible, net::SocketAddr, path::Path, rc::Rc};

pub type NativeApp = App<SqliteDb, FileStore>;
pub fn open_app(data: &Path, config: Config) -> Result<NativeApp> {
    std::fs::create_dir_all(data).map_err(|_| Error::storage())?;
    if config.admin_secret.len() < 32 {
        return Err(Error::invalid(
            "Admin secret must contain at least 32 bytes.",
        ));
    }
    Ok(App::new(
        SqliteDb::open(data.join("metadata.sqlite"))?,
        FileStore::open(data.join("objects"))?,
        config,
    ))
}
// One executor thread; requests yield at I/O boundaries. SQLite batches remain atomic.
pub async fn serve(listener: tokio::net::TcpListener, app: Rc<NativeApp>) -> std::io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tokio::task::spawn_local(async move {
            let service = service_fn(move |req: hyper::Request<Incoming>| {
                let app = app.clone();
                async move { Ok::<_, Infallible>(respond(app, req).await) }
            });
            let _ = http1::Builder::new()
                .max_buf_size(32 * 1024)
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}
async fn respond(
    app: Rc<NativeApp>,
    req: hyper::Request<Incoming>,
) -> hyper::Response<Full<Bytes>> {
    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let mut headers = BTreeMap::new();
    let mut valid = true;
    for name in req.headers().keys() {
        if [
            "authorization",
            "origin",
            "content-type",
            "mcp-protocol-version",
        ]
        .contains(&name.as_str())
            && req.headers().get_all(name).iter().count() != 1
        {
            valid = false
        }
        match req.headers().get(name).and_then(|v| v.to_str().ok()) {
            Some(v) => {
                headers.insert(name.as_str().to_owned(), v.to_owned());
            }
            None => valid = false,
        }
    }
    let max = if matches!(
        crate::http::classify(&method, &path),
        crate::http::Route::UploadContent(_)
    ) {
        app.limits.max_blob_bytes as usize
    } else {
        app.limits.max_json_bytes
    };
    let response = if !valid {
        Response::json(
            400,
            serde_json::json!({"ok":false,"error":{"code":"INVALID_ARGUMENT","message":"Invalid or repeated request header."}}),
        )
    } else {
        match Limited::new(req.into_body(), max).collect().await {
            Ok(body) => {
                app.handle(Request {
                    method,
                    path,
                    headers,
                    body: body.to_bytes().to_vec(),
                })
                .await
            }
            Err(_) => Response::json(
                413,
                serde_json::json!({"ok":false,"error":{"code":"RESOURCE_LIMIT","message":"Request body exceeds the deployment limit."}}),
            ),
        }
    };
    let mut builder = hyper::Response::builder()
        .status(response.status)
        .header("x-content-type-options", "nosniff");
    for (k, v) in response.headers {
        builder = builder.header(k, v)
    }
    builder
        .body(Full::new(Bytes::from(response.body)))
        .unwrap_or_else(|_| hyper::Response::new(Full::new(Bytes::new())))
}
pub async fn listen(address: SocketAddr, app: Rc<NativeApp>) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(address).await?;
    tokio::select! {r=serve(listener,app)=>r,_=tokio::signal::ctrl_c()=>Ok(())}
}
