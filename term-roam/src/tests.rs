use std::io::{Read, Write};
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::process::Command;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// An agent on loopback whose connections run `program` with the client's arguments.
fn agent(program: &'static str) -> Ticket {
    let local = Ipv4Addr::LOCALHOST;
    let (server, ticket) = bind((local, 0).into(), local.into()).expect("bind");
    let spawn: Arc<server::Spawn> = Arc::new(move |args: &[String]| {
        let mut command = Command::new(program);
        command.args(args);
        command
    });
    thread::spawn(move || server.run(spawn, Duration::from_secs(2)));
    ticket
}

fn read_line(reader: &mut Reader) -> String {
    let mut line = Vec::new();
    let mut byte = [0u8];
    while reader.read(&mut byte).expect("read") == 1 && byte[0] != b'\n' {
        line.push(byte[0]);
    }
    String::from_utf8(line).expect("utf-8")
}

#[test]
fn a_ticket_is_found_after_what_a_login_prints() {
    let ticket = Ticket {
        addr: "192.0.2.1:60001".parse().unwrap(),
        fingerprint: [7; 32],
        token: "secret".to_owned(),
    };
    let output = format!("Welcome to thalia\n{}\n", ticket.line());
    assert_eq!(Ticket::find(&output), Some(ticket));
    assert_eq!(Ticket::find("rune-term-agent 2 192.0.2.1:1 00 x"), None);
}

#[test]
fn the_stream_follows_the_client_to_a_new_socket() {
    let ticket = agent("cat");
    let mut client = connect(&ticket, &[], WAIT).expect("connect");
    client.writer.write_all(b"before\n").unwrap();
    assert_eq!(read_line(&mut client.reader), "before");
    let moved = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    client.rebind(moved).expect("rebind");
    client.writer.write_all(b"after\n").unwrap();
    assert_eq!(read_line(&mut client.reader), "after");
}

#[test]
fn the_agent_runs_its_program_with_the_clients_arguments() {
    let ticket = agent("echo");
    let mut client = connect(&ticket, &["-C".to_owned(), "a b".to_owned()], WAIT).expect("connect");
    assert_eq!(read_line(&mut client.reader), "-C a b");
    let mut rest = Vec::new();
    client.reader.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty(), "the stream ends with the program");
}

#[test]
fn a_wrong_token_or_certificate_is_refused() {
    let ticket = agent("cat");
    let wrong = Ticket {
        token: "0".repeat(64),
        ..ticket.clone()
    };
    let refused = connect(&wrong, &[], WAIT).err().expect("refused");
    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
    let forged = Ticket {
        fingerprint: [0; 32],
        ..ticket
    };
    assert!(connect(&forged, &[], WAIT).is_err());
}
