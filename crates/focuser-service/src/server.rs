use crate::authority::Authority;
use focuser_ipc::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncReadExt;

struct TickLifetime(Arc<AtomicBool>);
impl Drop for TickLifetime {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

async fn respond(
    pipe: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    result: Result<ReplyBody, IpcError>,
) {
    let delivered = write_frame(
        pipe,
        &Reply {
            version: VERSION,
            result,
        },
    )
    .await;
    let delivered = match delivered {
        Err(error) if error.code == "message_too_large" => {
            write_frame(
                pipe,
                &Reply {
                    version: VERSION,
                    result: Err(error),
                },
            )
            .await
        }
        result => result,
    };
    if delivered.is_ok() {
        // DisconnectNamedPipe discards unread output; Tokio flush is not a
        // delivery acknowledgement. Wait for consumption, with a deadline.
        let _ = tokio::time::timeout(IO_TIMEOUT, pipe.read_u8()).await;
    }
}

/// Policy ticks never depend on accepting/servicing a pipe connection. Keep a
/// listener handle alive across requests; reconnect early-disconnected clients
/// and reject overload without terminating enforcement.
pub async fn serve(
    authority: Arc<Authority>,
    name: String,
    production: bool,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), IpcError> {
    let running = Arc::new(AtomicBool::new(true));
    let runtime_running = running.clone();
    let runtime_authority = authority.clone();
    let mut runtime = tokio::task::spawn_blocking(move || {
        while runtime_running.load(Ordering::SeqCst) {
            if let Err(error) = runtime_authority.tick() {
                tracing::error!(%error,"Authority tick failed");
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    let _lifetime = TickLifetime(running.clone());
    let start = async {
        loop {
            match windows::create_server(&name, &authority.config.owner_sid, production, true) {
                Ok(pipe) => break Ok(pipe),
                Err(error) if production => {
                    // An occupied pipe name is a disconnected control surface,
                    // never permission to stop policy ticks or create a new DB.
                    tracing::error!(%error,"Authority pipe unavailable; enforcement continues");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(error) => break Err(error),
            }
        }
    };
    let mut listener = tokio::select! {
        result=start=>match result {
            Ok(pipe)=>pipe,
            Err(error)=>{running.store(false,Ordering::SeqCst);let _=runtime.await;return Err(error);}
        },
        _=stop.changed()=>{running.store(false,Ordering::SeqCst);let _=runtime.await;return Ok(());},
        _=&mut runtime=>return Err(IpcError::new("internal","Policy runtime exited unexpectedly")),
    };
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    let mut workers = tokio::task::JoinSet::new();
    let mut runtime_ended = false;
    let result = loop {
        tokio::select! {
            _=stop.changed()=>break Ok(()),
            _=&mut runtime=>{runtime_ended=true;break Err(IpcError::new("internal","Policy runtime exited unexpectedly"));},
            connected=listener.connect()=>{
                if let Err(error)=connected {
                    tracing::warn!(%error,"Pipe client disconnected before accept");
                    let _=listener.disconnect();
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;continue;
                }
                // Reserve capacity BEFORE creating the next instance. Otherwise
                // the 17th connection can exhaust the OS instance limit.
                let Ok(permit)=permits.clone().try_acquire_owned() else {
                    // Consume the framed request before waiting for its reply
                    // acknowledgement; queued request bytes are not an ACK.
                    let _=read_frame::<Request>(&mut listener).await;
                    respond(&mut listener,Err(IpcError::new("busy","Authority connection limit reached"))).await;
                    let _=listener.disconnect();continue;
                };
                let next=match windows::create_server(&name,&authority.config.owner_sid,production,false) {
                    Ok(next)=>next,
                    Err(error)=>{
                        tracing::error!(%error,"Could not create pipe instance; enforcement continues");
                        let _=read_frame::<Request>(&mut listener).await;
                        respond(&mut listener,Err(IpcError::new("busy","Pipe instance unavailable"))).await;
                        let _=listener.disconnect();drop(permit);continue;
                    }
                };
                let mut pipe=std::mem::replace(&mut listener,next);let authority=authority.clone();
                workers.spawn(async move {
                    let _permit=permit;
                    let result=match read_frame::<Request>(&mut pipe).await {
                        Ok(request)=>match windows::authenticate_client(&pipe,&authority.config.owner_sid) {
                            Ok(peer)=>tokio::task::spawn_blocking(move ||authority.handle(&peer,request)).await
                                .unwrap_or_else(|_|Err(IpcError::new("internal","Request worker failed"))),
                            Err(error)=>Err(error),
                        },
                        Err(error)=>Err(error),
                    };
                    respond(&mut pipe,result).await;let _=pipe.disconnect();
                });
                while workers.len()>MAX_CONNECTIONS*2 {let _=workers.join_next().await;}
            },
            Some(_)=workers.join_next(),if !workers.is_empty()=>{}
        }
    };
    workers.abort_all();
    running.store(false, Ordering::SeqCst);
    if !runtime_ended {
        let _ = runtime.await;
    }
    result
}
