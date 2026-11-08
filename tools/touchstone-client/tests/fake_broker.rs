//! Native-only: opens local TCP sockets.  A stand-in broker speaks the touchstone/1 frames: it answers every
//! call of function `ID` with its first argument and refuses function `NO`.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use touchstone_client::{Answer, Arg, Client, Error, Request};

fn read_frame(stream: &mut TcpStream) -> Option<(u8, Value)> {
    let mut size = [0u8; 4];
    stream.read_exact(&mut size).ok()?;
    let size = u32::from_be_bytes(size) as usize;
    let mut body = vec![0u8; size];
    stream.read_exact(&mut body).ok()?;
    Some((body[0], serde_json::from_slice(&body[1..]).unwrap()))
}

fn write_frame(stream: &mut TcpStream, kind: u8, body: &Value) {
    let text = serde_json::to_vec(body).unwrap();
    stream
        .write_all(&((text.len() + 1) as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&[kind]).unwrap();
    stream.write_all(&text).unwrap();
}

fn serve(listener: TcpListener) {
    let (mut stream, _) = listener.accept().unwrap();
    let mut refuse = std::collections::HashSet::new();
    while let Some((kind, body)) = read_frame(&mut stream) {
        match kind {
            b'H' => {
                assert_eq!(body["token"], json!("secret"));
                write_frame(
                    &mut stream,
                    b'H',
                    &json!({"v": "touchstone/1", "server": "fake", "envs": {"e1": {"build": "7"}}}),
                );
            }
            b'S' => {
                let id = body["id"].as_str().unwrap().to_string();
                let mut answers = Vec::new();
                for g in body["groups"].as_array().unwrap() {
                    if g["f"] == "NO" {
                        refuse.insert(id.clone());
                        continue;
                    }
                    let width: usize = g["k"].as_str().unwrap().split(',').count();
                    let n = g["n"].as_u64().unwrap() as usize;
                    let v = g["v"].as_array().unwrap();
                    let values: Vec<Value> = (0..n).map(|i| v[i * width].clone()).collect();
                    let mut answer = json!({"ty": "n".repeat(n), "v": values});
                    if let Some(o) = g.get("o") {
                        answer["o"] = o.clone();
                    } else {
                        answer["i"] = g["i"].clone();
                    }
                    answers.push(answer);
                }
                if refuse.contains(&id) {
                    write_frame(
                        &mut stream,
                        b'X',
                        &json!({"id": id, "code": "refused", "detail": "no such function"}),
                    );
                    continue;
                }
                write_frame(
                    &mut stream,
                    b'A',
                    &json!({"id": id, "seq": body["seq"], "env": "e1", "g": answers}),
                );
                if body["last"] == json!(true) {
                    write_frame(&mut stream, b'D', &json!({"id": id, "calls": 0}));
                }
            }
            _ => {}
        }
    }
}

fn client_for(listener: &TcpListener) -> Client {
    let port = listener.local_addr().unwrap().port();
    let path = std::env::temp_dir().join(format!("touchstone-endpoint-{port}.json"));
    std::fs::write(
        &path,
        json!({"host": "127.0.0.1", "port": port, "token": "secret"}).to_string(),
    )
    .unwrap();
    let client = Client::connect_endpoint(&path);
    let _ = std::fs::remove_file(&path);
    client.expect("connect to the stand-in broker")
}

#[test]
fn ask_returns_answers_in_request_order_across_frames_and_shapes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = {
        let l2 = listener.try_clone().unwrap();
        let server = thread::spawn(move || serve(l2));
        let client = client_for(&listener);
        std::mem::forget(server);
        client
    };
    let mut rows: Vec<Vec<Arg>> = Vec::new();
    for i in 0..9000u64 {
        rows.push(vec![Arg::number_bits(0x3ff0000000000000 + i)]);
        if i % 1000 == 0 {
            rows.push(vec![Arg::Text(format!("t{i}"))]);
        }
    }
    let reply = client.ask("ID", rows.clone()).expect("answered");
    assert_eq!(reply.rows.len(), rows.len());
    for (row, answer) in rows.iter().zip(&reply.rows) {
        match (&row[0], answer) {
            (Arg::Number(bits), Answer::Number(got)) => assert_eq!(bits, got),
            (Arg::Text(_), Answer::Number(_)) => {} // the stand-in answers text rows with the number letter
            other => panic!("unexpected pair {other:?}"),
        }
    }
    assert_eq!(reply.env_id.as_deref(), Some("e1"));
    assert_eq!(reply.env.as_ref().unwrap()["build"], json!("7"));
}

#[test]
fn a_refusal_comes_back_as_a_service_error_and_many_requests_run_together() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let l2 = listener.try_clone().unwrap();
    thread::spawn(move || serve(l2));
    let client = client_for(&listener);
    let replies = client.ask_many(vec![
        Request::new("ID", vec![vec![Arg::number(1.0)]]),
        Request::new("NO", vec![vec![Arg::number(2.0)]]),
        Request::new("ID", vec![vec![Arg::number(3.0)], vec![Arg::number(4.0)]]),
    ]);
    assert_eq!(
        replies[0].as_ref().unwrap().rows,
        [Answer::Number(1.0f64.to_bits())]
    );
    assert!(matches!(&replies[1], Err(Error::Service { code, .. }) if code == "refused"));
    assert_eq!(replies[2].as_ref().unwrap().rows.len(), 2);
}
