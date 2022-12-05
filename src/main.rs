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
use crate::port_operations::{flush_port, serial_readline, serial_write, unlogged_serial_readline, unlogged_serial_write};

mod macros;
mod message;
mod config;
mod port_operations;

struct Controller {
    router_port: Box<dyn SerialPort>,
    pump_port: Box<dyn SerialPort>,
    application_port: Box<dyn SerialPort>,
    slot_occupancy: u64,
}

impl Controller {
    pub fn router_execute(&mut self, command: &str) -> ControlFlow<String> {
        serial_write(&mut self.router_port, command);
        if serial_readline(&mut self.router_port, "\r\n") == "G1:OK" {
            return ControlFlow::Continue(());
        }
        ControlFlow::Break(format!("Router - error executing command: [{command}]"))
    }

    pub fn pump_execute(&mut self, command: &str) -> ControlFlow<String> {
        flush_port(&mut self.pump_port);
        serial_write(&mut self.pump_port, command);
        sleep(Duration::from_secs(1));
        await_pump_availability(&mut self.pump_port)
    }

    pub fn pump_execute_async(&mut self, command: &str) -> ControlFlow<String> {
        flush_port(&mut self.pump_port);
        serial_write(&mut self.pump_port, command);
        return ControlFlow::Continue(());
    }
}

fn await_pump_availability(pump_port: &mut Box<dyn SerialPort>) -> ControlFlow<String> {
    loop {
        unlogged_serial_write(pump_port, "/1Q29\r\n");
        let mut status = unlogged_serial_readline(pump_port, "\r\n");
        status.remove(0);
        status.pop();
        let is_free = status == "/0c";
        if is_free {
            return ControlFlow::Continue(());
        }
        sleep(Duration::from_secs(1));
    }
}

fn execute_command(ports: &mut Controller, command: &str) -> ControlFlow<String> {
    await_pump_availability(&mut ports.pump_port)?;
    let command_type = command.split('_').next().expect("Cannot get command type");
    match command_type {
        "LA" => handle_liquid_application(ports, command),
        "W" => handle_waiting_command(command),
        "TC" => {
            log::error!("PRETENDING TO DO TEMP CHANGE");
            ControlFlow::Continue(())
        }
        "BTC" => {
            log::error!("PRETENDING TO DO TEMP CHANGE");
            ControlFlow::Continue(())
        }
        _ => ControlFlow::Break("Unknown Command ".to_string().add(command))
    }
}

fn handle_temperature_change(controller: &Controller, command: &str) {}

fn handle_liquid_application(controller: &mut Controller, command: &str) -> ControlFlow<String> {
    log::trace!("Executing liquid application {}", command);
    flush_port(&mut controller.router_port);
    flush_port(&mut controller.pump_port);
    log::trace!("Slot occupancy - {}", controller.slot_occupancy);
    if controller.slot_occupancy > 0 {
        log::trace!("Pumping liquid out of slot");
        let vol = microliter_to_pumpunit(controller.slot_occupancy);
        controller.pump_execute(&*format!("/2gI1A12000O2A0G4R\r\n"))?;
        controller.slot_occupancy = 0;
    }

    let parts: Vec<&str> = command.split('_').collect();
    let from = unwrap_option!(parts.get(1), "Cannot deduce 'from' part".to_string());
    let [x, y, z] = CONFIG.tube_holder_coordinates.get(&from.to_string())
        .map(|coords| coords.split(":").collect::<Vec<&str>>())
        .and_then(|coords| <[&str; 3]>::try_from(coords).ok())
        .expect(format!("Couldn't find x/y/z coordinates from command: {command}").as_str());
    let from_number = unwrap_result!(from.parse::<u64>());
    let vol_microliter = parts.get(3)
        .and_then(|v| v.parse().ok())
        .unwrap();
    if from_number > 33 {
        return handle_external_liquid_application(controller, from_number, vol_microliter);
    }

    controller.router_execute(&*format!("G1X{x}Y{y}Z{z}\r\n"))?;
    let vol: u64 = microliter_to_pumpunit(vol_microliter);

    log::trace!("Taking liquid");
    controller.pump_execute(&*format!("/1I1A{vol}O2A0R\r\n"))?;
    controller.router_execute(&*format!("G1X{x}Y{y}Z0\r\n"))?;
    log::trace!("Pumping liquid");
    // controller.pump_execute_async("/2gI1A12000O2A0G7R\r\n")?; // Using other pump to pump out liquid from slot
    controller.pump_execute(&*format!("/1gI1A12000O2A0G12R\r\n"))?; // pumping to slot
    controller.slot_occupancy = vol_microliter;
    if CONFIG.constant_cleaning == false {
        return ControlFlow::Continue(());
    }
    log::trace!("Starting water cleaning");
    controller.router_execute("G1X227Y152Z-20\r\n")?;
    log::trace!("Pumping water");
    controller.pump_execute("/1gI4A12000O1A0G2R\r\n")?;
    log::trace!("Pumping Air");
    controller.pump_execute("/1gI5A12000O1A0G4R\r\n")?;
    ControlFlow::Continue(())
}

fn handle_external_liquid_application(controller: &mut Controller, from: u64, vol: u64) -> ControlFlow<String> {
    let required_channel = match from {
        34 => 4,
        35 => 7,
        36 => 6,
        _ => return ControlFlow::Break("Developer is dumb".to_string())
    };
    let pump_vol = microliter_to_pumpunit(vol);
    controller.pump_execute(&*format!("/1I{required_channel}A{pump_vol}O1A0R\r\n"))?;
    // controller.pump_execute_async("/2gI1A12000O2A0G3R\r\n")?;
    controller.pump_execute("/1gI4A12000O1A0G2R\r\n")?;
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

fn handle_line(ports: &mut Controller, line: String) {
    let msg = message::parse_to_message(line.clone());
    match msg {
        Some(v) => handle_message(ports, v),
        None => log::error!("Invalid message: {}", line),
    }
}


fn handle_message(ports: &mut Controller, msg: Message) {
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

fn microliter_to_pumpunit(microliters: u64) -> u64 {
    let res = microliters * 24;
    if res > 12000 {
        log::error!("Calculated pump units over 12000")
    }
    res
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
    let mut controller = Controller {
        application_port: serialport::new(CONFIG.application_port_path.as_str(), 9600).open().unwrap(),
        pump_port: serialport::new(CONFIG.pump_port_path.as_str(), 9600).open().unwrap(),
        router_port: serialport::new(CONFIG.router_port_path.as_str(), 115200).open().unwrap(),
        slot_occupancy: 0,
    };

    flush_port(&mut controller.router_port);
    sleep(Duration::from_secs(5));
    serial_readline(&mut controller.router_port, "\r\n"); // read setup done
    serial_write(&mut controller.router_port, "G28\r\n");
    serial_write(&mut controller.pump_port, "/1ZgI4A12000O3A0G3R\r\n");
    serial_write(&mut controller.pump_port, "/2ZR\r\n");
    serial_readline(&mut controller.router_port, "\r\n");
    // ROUTER INIT: "G28\n\r" and then wait (10 sec)
    // PUMP INIT: "/1ZR\n\r"
    loop {
        let line = serial_readline(&mut controller.application_port, "\n");
        handle_line(&mut controller, line)
    }
}
