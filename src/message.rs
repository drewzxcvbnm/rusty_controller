use crate::unwrap_or_none;

pub struct Message {
    pub channel: i8,
    pub data: String,
    pub crc: u32,
}

pub fn parse_to_message(line: String) -> Option<Message> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let channel: i8 = unwrap_or_none!(parts.get(0).unwrap().parse());
    let data: String = parts.get(1).unwrap().to_string();
    let crc: u32 = unwrap_or_none!(u32::from_str_radix(parts.get(2).unwrap(), 16));
    if crc32fast::hash(data.as_bytes()) != crc {
        log::error!("Invalid CRC");
        return None;
    }
    return Option::from(Message { channel, data, crc });
}

