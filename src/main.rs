use std::env;
use tide_websockets::{WebSocket, Message};
use prometheus::{TextEncoder, Encoder, register_int_counter, IntCounter, register_int_gauge, IntGauge};
use anyhow::Result;
use tide::Body;
use std::sync::{atomic::{AtomicI64, Ordering}, Arc};
use tide_websockets::WebSocketConnection;
use async_std::prelude::*;
use memchr::memchr;
use std::time::Duration;
use async_std::task;
use async_std::fs::{read_to_string, write};
use lazy_static::lazy_static;

lazy_static! {
    static ref OPS_COUNTER: IntCounter = register_int_counter!("incdec_ops", "IncDec operations executed").unwrap();
    static ref COUNTER_GAUGE: IntGauge = register_int_gauge!("incdec_counter", "Current counter value").unwrap();
    static ref CONNECTIONS_COUNTER: IntCounter = register_int_counter!("incdec_total_conns", "Total client connections made").unwrap();
}

fn set_mime_str(s: &'static str, mime: &'static str) -> Body {
    let mut b: Body = s.into();
    b.set_mime(mime);
    b
}

type Counter = Arc<AtomicI64>;

fn update_counter(ctr: &Counter, by: i64) -> i64 {
    let new_val = ctr.fetch_add(by, Ordering::Relaxed);
    OPS_COUNTER.inc();
    COUNTER_GAUGE.set(new_val);
    new_val
}

async fn handle_connection(ws: WebSocketConnection, ctr: Counter) -> Result<()> {
    CONNECTIONS_COUNTER.inc();
    let mut counter_val = ctr.load(Ordering::Relaxed);
    ws.send_string(format!("{}", counter_val)).await?;
    let ctr_ = ctr.clone();
    let mut ws_ = ws.clone();
    let ws_link = async move {
        while let Some(Ok(Message::Text(txt))) = ws_.next().await {
            if let Some(_) = memchr(b'i', txt.as_bytes()) {
                // increment
                update_counter(&ctr_, 1);
            }
            else if let Some(_) = memchr(b'd', txt.as_bytes()) {
                // decrement
                update_counter(&ctr_, -1);
            }
        }
        let r: Result<()> = Ok(());
        r
    };
    let poll_value = async move {
        loop {
            task::sleep(Duration::from_millis(50)).await;
            let new = ctr.load(Ordering::Relaxed);
            if counter_val != new {
                ws.send_string(format!("{}", new)).await?;
                counter_val = new;
            }
        }
    };
    ws_link.race(poll_value).await?;
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let save_path = args[1].clone();
    let port = args[2].parse::<u16>()?;

    let counter = Arc::new(AtomicI64::new({
        match read_to_string(&save_path).await {
            Ok(s) => s.parse::<i64>()?,
            Err(_) => 0
        }
    }));
    let counter_ = counter.clone();

    let mut app = tide::with_state(counter);
    app.at("/api").get(WebSocket::new(|req: tide::Request<Counter>, stream| {
        let state = req.state().clone();
        async move { 
            handle_connection(stream, state).await.map_err(|_| tide::Error::from_str(tide::StatusCode::InternalServerError, "should not occur"))
        }
    }));
    app.at("/metrics").get(|_req| async {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        encoder.encode(&metric_families, &mut buffer)?;
        let mut b = Body::from_bytes(buffer);
        b.set_mime("text/plain");
        Ok(b)
    });
    app.at("/inc").post(|req: tide::Request<Counter>| async move {
        let mut b: Body = update_counter(req.state(), 1).to_string().into();
        b.set_mime("text/plain");
        Ok(b)
    });
    app.at("/dec").post(|req: tide::Request<Counter>| async move {
        let mut b: Body = update_counter(req.state(), -1).to_string().into();
        b.set_mime("text/plain");
        Ok(b)
    });
    app.at("/").get(|_req| async { Ok(set_mime_str(include_str!("../index.html"), "text/html")) });

    println!("Running on port {}", port);

    task::spawn(async move {
        loop {
            write(&save_path, format!("{}", counter_.load(Ordering::Relaxed))).await.unwrap();
            task::sleep(Duration::from_secs(10)).await;
        }
    });

    app.listen(("0.0.0.0", port)).await?;

    Ok(())
}