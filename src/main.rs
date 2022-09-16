use std::io::{BufRead, Read, Write};
use std::ops::{Add, ControlFlow};
use std::process::Command;
use std::ptr::null;
use std::thread::sleep;
use std::time::Duration;

use serialport::SerialPort;
use simple_logger::SimpleLogger;
use sysinfo::{ProcessExt, SystemExt};

use message::Message;

use crate::consts::{PUMP_SERIAL_PORT, ROUTER_SERIAL_PORT, USER_APPLICATION_SERIAL_PORT};

mod consts;
mod macros;
mod message;

static mut ROUTER_PORT: Option<Box<dyn SerialPort>> = Option::None;

trait CommandExecutor {
    fn execute_command(&mut self, command: &str) -> ControlFlow<String>;
    fn handle_liquid_application(&mut self, command: &str) -> ControlFlow<String>;
    fn handle_waiting_command(&self, command: &str) -> ControlFlow<String>;
    fn handle_line(&mut self, line: String);
    fn handle_message(&mut self, msg: Message);
}

struct Controller {
    router_port: Box<dyn SerialPort>,
    pump_port: Box<dyn SerialPort>,
    application_port: Box<dyn SerialPort>,
}

impl CommandExecutor for Controller {
    fn execute_command(&mut self, command: &str) -> ControlFlow<String> {
        let command_type = command.split('_').next().expect("Cannot get command type");
        match command_type {
            "LA" => self.handle_liquid_application(command),
            "W" => self.handle_waiting_command(command),
            "TC" => ControlFlow::Break("Unimplemented Command TC".to_string()),
            "BTC" => ControlFlow::Break("Unimplemented Command BTC".to_string()),
            _ => ControlFlow::Break("Unknown Command ".to_string().add(command))
        }
    }

    fn handle_liquid_application(&mut self, command: &str) -> ControlFlow<String> {
        log::trace!("Executing liquid application {}", command);
        let parts: Vec<&str> = command.split('_').collect();

        if let [x, y, z] = parts.get(1).expect("").split(':').collect::<Vec<&str>>()[..] {
            serial_write(&mut self.router_port, &*format!("G1X{}Y{}Z{}", x, y, z));
            // read back G1:OK\n\r

            // Pump sucks in liquid
            // /1I0A<6000>O1A0
        } else {
            log::error!("Invalid liquid coordinates");
            return ControlFlow::Break("Invalid liquid coordinates".to_string());
        }
        ControlFlow::Continue(())
    }

    fn handle_waiting_command(&self, command: &str) -> ControlFlow<String> {
        let parts: Vec<&str> = command.split('_').collect();
        let time: u64 = parts.get(1)
            .and_then(|t| t.parse().ok())
            .expect("Cannot get time for wait command");
        log::info!("Waiting for {} milliseconds", time);
        sleep(Duration::from_millis(time));
        ControlFlow::Continue(())
    }

    fn handle_line(&mut self, line: String) {
        let msg = message::parse_to_message(line.clone());
        match msg {
            Some(v) => self.handle_message(v),
            None => log::error!("Invalid message: {}", line),
        }
    }


    fn handle_message(&mut self, msg: Message) {
        log::trace!("Parsed message: {}, {}, {}", msg.channel, msg.data, msg.crc);
        if msg.channel != 4 {
            return;
        }
        msg.data.split(' ').try_for_each(|c| self.execute_command(c));
        return;
    }
}

fn escape_chars(st: &str) -> String {
    st.replace("\n", "\\n").replace("\r", "\\r")
}

fn serial_write(port: &mut Box<dyn SerialPort>, msg: &str) {
    let port_name = port.name().unwrap();
    log::trace!("Writing to port {}: {}", port_name, escape_chars(msg));
    port.write(msg.as_ref());
}

fn serial_readline(port: &mut Box<dyn SerialPort>, end_delimiter: &str) -> String {
    let mut line = String::new();
    loop {
        let mut buf: [u8; 1] = [0];
        if port.bytes_to_read().unwrap() != 0 {
            port.read(&mut buf);
            line.push(char::from(buf[0]));
        } else {
            sleep(Duration::from_micros(10));
            continue;
        }
        if line.ends_with(end_delimiter) {
            log::trace!("Got {} from port {}", escape_chars(&line), port.name().unwrap());
            return line.strip_suffix(end_delimiter).unwrap().to_string();
        }
    }
}

fn test_env_setup() {
    sysinfo::System::new_all()
        .processes_by_name("socat")
        .for_each(|p| { p.kill(); });
    Command::new("socat").args(["-d", "-d", "pty,raw,echo=1,link=/tmp/app1", "pty,raw,echo=1,link=/tmp/app2"])
        .spawn().ok();
    Command::new("socat").args(["-d", "-d", "pty,raw,echo=1,link=/tmp/pump1", "pty,raw,echo=1,link=/tmp/pump2"])
        .spawn().ok();
    Command::new("socat").args(["-d", "-d", "pty,raw,echo=1,link=/tmp/router1", "pty,raw,echo=1,link=/tmp/router2"])
        .spawn().ok();
    sleep(Duration::from_secs(1));
}

fn main() {
    SimpleLogger::new().init().unwrap();
    test_env_setup();
    let mut controller = Controller {
        application_port: serialport::new(USER_APPLICATION_SERIAL_PORT, 9600).open().unwrap(),
        pump_port: serialport::new(PUMP_SERIAL_PORT, 9600).open().unwrap(),
        router_port: serialport::new(ROUTER_SERIAL_PORT, 9600).open().unwrap(),
    };
    serial_write(&mut controller.router_port, "G28\n\r");
    serial_write(&mut controller.pump_port, "/1ZR\n\r");
    // ROUTER INIT: "G28\n\r" and then wait (10 sec)
    // PUMP INIT: "/1ZR\n\r"
    loop {
        let line = serial_readline(&mut controller.application_port, "\n");
        controller.handle_line(line)
    }
}
