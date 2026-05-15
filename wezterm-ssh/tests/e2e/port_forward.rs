use crate::sshd::*;
use rstest::*;
use std::io::{Read, Write};
use wezterm_ssh::PortForward;

#[rstest]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[cfg_attr(not(feature = "libssh-rs"), ignore)]
fn local_port_forwarding_should_work(#[future] session: SessionWithSshd) {
    if !sshd_available() {
        return;
    }
    smol::block_on(async {
        let session: SessionWithSshd = session.await;

        // 1. Start a target TCP listener
        let target_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target_addr = target_listener.local_addr().unwrap();
        target_listener.set_nonblocking(true).unwrap();

        // 2. Request port forwarding
        let bound_addr = session
            .add_port_forward(PortForward::Local {
                local_host: "127.0.0.1".to_string(),
                local_port: 0,
                remote_host: target_addr.ip().to_string(),
                remote_port: target_addr.port(),
            })
            .await
            .unwrap();

        // 3. Connect to the bound local port
        let mut client_socket = std::net::TcpStream::connect(bound_addr).unwrap();
        client_socket.write_all(b"hello").unwrap();

        // 4. Accept connection on target listener and verify data
        let mut server_socket = loop {
            match target_listener.accept() {
                Ok((sock, _)) => break sock,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    smol::Timer::after(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => panic!("Error accepting: {}", e),
            }
        };

        let mut buf = [0u8; 5];
        server_socket.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
    })
}

#[rstest]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[cfg_attr(not(feature = "libssh-rs"), ignore)]
fn local_port_forwarding_should_fail_if_target_not_listening(#[future] session: SessionWithSshd) {
    if !sshd_available() {
        return;
    }
    smol::block_on(async {
        let session: SessionWithSshd = session.await;

        // 1. Find a port that is likely NOT listening
        let unused_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let unused_addr = unused_listener.local_addr().unwrap();
        drop(unused_listener); // now it's not listening

        // 2. Request port forwarding to that unused port
        let bound_addr = session
            .add_port_forward(PortForward::Local {
                local_host: "127.0.0.1".to_string(),
                local_port: 0,
                remote_host: unused_addr.ip().to_string(),
                remote_port: unused_addr.port(),
            })
            .await
            .unwrap();

        // 3. Connect to the bound local port
        let mut client_socket = std::net::TcpStream::connect(bound_addr).unwrap();

        // 4. Verify that the connection is closed immediately or data cannot be sent/read
        // Since the channel open fails, WezTerm should close the local socket.
        let mut buf = [0u8; 1];
        let res = client_socket.read(&mut buf);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 0); // EOF
    })
}

#[rstest]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[cfg_attr(not(feature = "libssh-rs"), ignore)]
fn remote_port_forwarding_should_work(#[future] session: SessionWithSshd) {
    if !sshd_available() {
        return;
    }
    smol::block_on(async {
        let session: SessionWithSshd = session.await;

        // 1. Start a target TCP listener on local machine
        let target_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let target_addr = target_listener.local_addr().unwrap();
        target_listener.set_nonblocking(true).unwrap();

        // 2. Request remote port forwarding
        // We ask the server to listen on a random port (0)
        // and forward to our target_addr.
        let bound_addr = session
            .add_port_forward(PortForward::Remote {
                remote_host: Some("127.0.0.1".to_string()),
                remote_port: 0,
                local_host: target_addr.ip().to_string(),
                local_port: target_addr.port(),
            })
            .await
            .unwrap();

        let bound_port = bound_addr.port();

        // 3. Connect to the bound port on the SSH server!
        // In tests, the SSH server is running locally.
        let mut client_socket =
            std::net::TcpStream::connect(format!("127.0.0.1:{}", bound_port)).unwrap();
        client_socket.write_all(b"hello from remote").unwrap();

        // 4. Accept connection on target listener and verify data
        let mut server_socket = loop {
            match target_listener.accept() {
                Ok((sock, _)) => break sock,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    smol::Timer::after(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => panic!("Error accepting: {}", e),
            }
        };

        let mut buf = [0u8; 17];
        server_socket.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello from remote");
    })
}
