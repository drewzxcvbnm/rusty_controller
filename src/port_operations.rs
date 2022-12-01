use std::thread::sleep;
use std::time::Duration;

use log::log;
use serialport::SerialPort;

use crate::escape_chars;

pub fn serial_write(port: &mut Box<dyn SerialPort>, msg: &str) {
    let port_name = port.name().unwrap();
    log::trace!("Writing to port {}: {}", port_name, escape_chars(msg));
    port.write(msg.as_ref()).map_err(|e| log::error!("FAILED WRITE: {}", e));
}

pub fn unlogged_serial_write(port: &mut Box<dyn SerialPort>, msg: &str) {
    let port_name = port.name().unwrap();
    port.write(msg.as_ref()).map_err(|e| log::error!("FAILED WRITE: {}", e));
}


pub fn flush_port(port: &mut Box<dyn SerialPort>) {
    loop {
        let mut buf: [u8; 1] = [0];
        if port.bytes_to_read().unwrap() != 0 {
            port.read(&mut buf);
        } else {
            return;
        }
    }
}

pub fn serial_readline(port: &mut Box<dyn SerialPort>, end_delimiter: &str) -> String {
    return _serial_readline(port, end_delimiter, |s| log::trace!("{}", s));
}

pub fn unlogged_serial_readline(port: &mut Box<dyn SerialPort>, end_delimiter: &str) -> String {
    return _serial_readline(port, end_delimiter, |_| {});
}

pub fn _serial_readline(port: &mut Box<dyn SerialPort>, end_delimiter: &str, logger: fn(s: String)) -> String {
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
            logger(format!("Got [{}] from port {}", escape_chars(&line), port.name().unwrap()));
            return line.strip_suffix(end_delimiter).unwrap().to_string();
        }
    }
}

