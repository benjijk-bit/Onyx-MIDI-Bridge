use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

/// Wraps the UDP socket used to talk to Onyx.
pub struct OscOut {
    socket: UdpSocket,
    target: String,
}

impl OscOut {
    pub fn new(target_ip: &str, target_port: u16) -> anyhow::Result<Self> {
        // Bind to an ephemeral local port for sending.
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self {
            socket,
            target: format!("{}:{}", target_ip, target_port),
        })
    }

    pub fn send_float(&self, address: &str, value: f32) -> anyhow::Result<()> {
        self.send(address, vec![OscType::Float(value)])
    }

    pub fn send_int(&self, address: &str, value: i32) -> anyhow::Result<()> {
        self.send(address, vec![OscType::Int(value)])
    }

    fn send(&self, address: &str, args: Vec<OscType>) -> anyhow::Result<()> {
        let packet = OscPacket::Message(OscMessage {
            addr: address.to_string(),
            args,
        });
        let bytes = encoder::encode(&packet)?;
        self.socket.send_to(&bytes, &self.target)?;
        Ok(())
    }
}

/// A single decoded incoming OSC value we care about (fader or button feedback from Onyx).
#[derive(Debug, Clone)]
pub struct IncomingOsc {
    pub address: String,
    pub float_value: Option<f32>,
    pub int_value: Option<i32>,
}

/// Starts a background thread listening for OSC packets from Onyx on `listen_port`,
/// calling `on_message` for every decoded message. Returns immediately;
/// the thread runs for the lifetime of the process (simple v1 design).
pub fn start_listener<F>(listen_port: u16, on_message: F) -> anyhow::Result<()>
where
    F: Fn(IncomingOsc) + Send + 'static,
{
    let socket = UdpSocket::bind(("0.0.0.0", listen_port))?;
    std::thread::spawn(move || {
        let mut buf = [0u8; rosc::decoder::MTU];
        loop {
            match socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                        handle_packet(packet, &on_message);
                    }
                }
                Err(e) => {
                    eprintln!("OSC listener socket error: {e}");
                    break;
                }
            }
        }
    });
    Ok(())
}

fn handle_packet<F>(packet: OscPacket, on_message: &F)
where
    F: Fn(IncomingOsc),
{
    match packet {
        OscPacket::Message(msg) => {
            let mut float_value = None;
            let mut int_value = None;
            if let Some(first) = msg.args.first() {
                match first {
                    OscType::Float(f) => float_value = Some(*f),
                    OscType::Int(i) => int_value = Some(*i),
                    _ => {}
                }
            }
            on_message(IncomingOsc {
                address: msg.addr,
                float_value,
                int_value,
            });
        }
        OscPacket::Bundle(bundle) => {
            for p in bundle.content {
                handle_packet(p, on_message);
            }
        }
    }
}
