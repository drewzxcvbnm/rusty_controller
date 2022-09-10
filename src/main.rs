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
    fn write_to_router(&mut self, msg: &str);
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
            self.write_to_router(&*format!("G1X{}Y{}Z{}", x, y, z));// Router goes to liquid
            // Pump sucks in liquid
            // Router goes to slot?
            // Pump sucks out liquid?
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

    fn write_to_router(&mut self, msg: &str) {
        log::trace!("Writing to router: {}", msg);
        self.router_port.write(msg.as_ref());
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
    let mut line = String::new();
    loop {
        let mut buf: [u8; 1] = [0];
        if controller.application_port.bytes_to_read().unwrap() != 0 {
            controller.application_port.read(&mut buf);
            line.push(char::from(buf[0]));
        } else {
            sleep(Duration::from_micros(10));
            continue;
        }
        if line.ends_with('\n') {
            line.pop();
            controller.handle_line(line.clone());
            line.clear();
        }
    }
}
