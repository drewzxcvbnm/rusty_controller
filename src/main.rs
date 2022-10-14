use std::io::{Read, Write};
use std::ops::{Add, ControlFlow};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use log::log;
use serialport::SerialPort;
use simple_logger::SimpleLogger;
use sysinfo::{ProcessExt, SystemExt};

use message::Message;

use crate::config::CONFIG;

mod macros;
mod message;
mod config;

struct ControllerPorts {
    router_port: Box<dyn SerialPort>,
    pump_port: Box<dyn SerialPort>,
    application_port: Box<dyn SerialPort>,
}

fn router_execute(router_port: &mut Box<dyn SerialPort>, command: &str) -> ControlFlow<String> {
    serial_write(router_port, command);
    if serial_readline(router_port, "\r\n") == "G1:OK" {
        return ControlFlow::Continue(());
    }
    ControlFlow::Break(format!("Router - error executing command: [{command}]"))
}

fn pump_execute(pump_port: &mut Box<dyn SerialPort>, command: &str) -> ControlFlow<String> {
    flush_port(pump_port);
    serial_write(pump_port, command);
    sleep(Duration::from_secs(1));
    await_pump_availability(pump_port)
}

fn await_pump_availability(pump_port: &mut Box<dyn SerialPort>) -> ControlFlow<String> {
    loop {
        serial_write(pump_port, "/1Q29\r\n");
        let mut status = serial_readline(pump_port, "\r\n");
        status.remove(0);
        status.pop();
        let is_free = status == "/0c";
        if is_free {
            return ControlFlow::Continue(());
        }
        sleep(Duration::from_secs(1));
    }
}

fn execute_command(ports: &mut ControllerPorts, command: &str) -> ControlFlow<String> {
    await_pump_availability(&mut ports.pump_port)?;
    let command_type = command.split('_').next().expect("Cannot get command type");
    match command_type {
        "LA" => handle_liquid_application(ports, command),
        "W" => handle_waiting_command(command),
        "TC" => ControlFlow::Break("Unimplemented Command TC".to_string()),
        "BTC" => ControlFlow::Break("Unimplemented Command BTC".to_string()),
        _ => ControlFlow::Break("Unknown Command ".to_string().add(command))
    }
}

fn handle_liquid_application(ports: &mut ControllerPorts, command: &str) -> ControlFlow<String> {
    log::trace!("Executing liquid application {}", command);
    flush_port(&mut ports.router_port);
    flush_port(&mut ports.pump_port);
    let parts: Vec<&str> = command.split('_').collect();
    let [x, y, z] = parts.get(1)
        .and_then(|from| CONFIG.tube_holder_coordinates.get(&from.to_string()))
        .map(|coords| coords.split(":").collect::<Vec<&str>>())
        .and_then(|coords| <[&str; 3]>::try_from(coords).ok())
        .expect(format!("Couldn't find x/y/z coordinates from command: {command}").as_str());
    // let [x, y, z] = <[&str; 3]>::try_from(parts).ok().expect("Cannot unpack x,y,z");

    router_execute(&mut ports.router_port, &*format!("G1X{x}Y{y}Z{z}\r\n"))?;

    let vol: u64 = parts.get(3)
        .and_then(|v| v.parse().ok())
        .map(microliter_to_pumpunit)
        .unwrap();

    log::trace!("Taking water");
    pump_execute(&mut ports.pump_port, &*format!("/1I1A{vol}O2A0R\r\n"))?;
    router_execute(&mut ports.router_port, &*format!("G1X{x}Y{y}Z0\r\n"))?;
    log::trace!("Pumping liquid");
    pump_execute(&mut ports.pump_port, &*format!("/1gI1A12000O2A0G6R\r\n"))?;
    if CONFIG.constant_cleaning == false {
        return ControlFlow::Continue(());
    }
    log::trace!("Starting water cleaning");
    router_execute(&mut ports.router_port, "G1X227Y152Z-20\r\n")?;
    log::trace!("Pumping water");
    pump_execute(&mut ports.pump_port, "/1gI4A12000O1A0G2R\r\n")?;
    log::trace!("Pumping Air");
    pump_execute(&mut ports.pump_port, "/1gI5A12000O1A0G4R\r\n")?;
    ControlFlow::Continue(())
}

fn handle_waiting_command(command: &str) -> ControlFlow<String> {
    let parts: Vec<&str> = command.split('_').collect();
    let time: u64 = parts.get(1)
        .and_then(|t| t.parse().ok())
        .expect("Cannot get time for wait command");
    log::info!("Waiting for {} milliseconds", time);
    sleep(Duration::from_millis(time));
    ControlFlow::Continue(())
}

fn handle_line(ports: &mut ControllerPorts, line: String) {
    let msg = message::parse_to_message(line.clone());
    match msg {
        Some(v) => handle_message(ports, v),
        None => log::error!("Invalid message: {}", line),
    }
}


fn handle_message(ports: &mut ControllerPorts, msg: Message) {
    log::trace!("Parsed message: {}, {}, {}", msg.channel, msg.data, msg.crc);
    if msg.channel != 4 {
        return;
    }
    match msg.data.split(' ').try_for_each(|c| execute_command(ports, c)) {
        ControlFlow::Continue(_) => log::info!("Executed command successfully"),
        ControlFlow::Break(e) => log::error!("ERROR: {}", escape_chars(e.as_str()))
    }
}

fn escape_chars(st: &str) -> String {
    st.replace("\n", "\\n").replace("\r", "\\r")
}

fn serial_write(port: &mut Box<dyn SerialPort>, msg: &str) {
    let port_name = port.name().unwrap();
    log::trace!("Writing to port {}: {}", port_name, escape_chars(msg));
    port.write(msg.as_ref()).map_err(|e| log::error!("FAILED WRITE: {}", e));
}

fn microliter_to_pumpunit(microliters: u64) -> u64 {
    let res = microliters * 24;
    if res > 12000 {
        log::error!("Calculated pump units over 12000")
    }
    res
}

fn flush_port(port: &mut Box<dyn SerialPort>) {
    loop {
        let mut buf: [u8; 1] = [0];
        if port.bytes_to_read().unwrap() != 0 {
            port.read(&mut buf);
        } else {
            return;
        }
    }
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
            log::trace!("Got [{}] from port {}", escape_chars(&line), port.name().unwrap());
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
    // Command::new("socat").args(["-d", "-d", "pty,raw,echo=1,link=/tmp/pump1", "pty,raw,echo=1,link=/tmp/pump2"])
    //     .spawn().ok();
    // Command::new("socat").args(["-d", "-d", "pty,raw,echo=1,link=/tmp/router1", "pty,raw,echo=1,link=/tmp/router2"])
    //     .spawn().ok();
    sleep(Duration::from_secs(1));
}


fn main() {
    SimpleLogger::new().init().unwrap();
    test_env_setup();
    let mut ports = ControllerPorts {
        application_port: serialport::new(CONFIG.application_port_path.as_str(), 9600).open().unwrap(),
        pump_port: serialport::new(CONFIG.pump_port_path.as_str(), 9600).open().unwrap(),
        router_port: serialport::new(CONFIG.router_port_path.as_str(), 115200).open().unwrap(),
    };
    flush_port(&mut ports.router_port);
    sleep(Duration::from_secs(5));
    serial_readline(&mut ports.router_port, "\r\n"); // read setup done
    serial_write(&mut ports.router_port, "G28\r\n");
    serial_write(&mut ports.pump_port, "/1ZR\r\n");
    serial_readline(&mut ports.router_port, "\r\n");
    // ROUTER INIT: "G28\n\r" and then wait (10 sec)
    // PUMP INIT: "/1ZR\n\r"
    loop {
        let line = serial_readline(&mut ports.application_port, "\n");
        handle_line(&mut ports, line)
    }
}
