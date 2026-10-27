//! USB/IP wire format (protocol version 1.1.1). All fields are big-endian.

use std::io::{self, Read};

pub const PROTOCOL_VERSION: u16 = 0x0111;

pub const OP_REQ_IMPORT: u16 = 0x8003;
pub const OP_REP_IMPORT: u16 = 0x0003;
pub const OP_REQ_DEVLIST: u16 = 0x8005;
pub const OP_REP_DEVLIST: u16 = 0x0005;

pub const CMD_SUBMIT: u32 = 1;
pub const CMD_UNLINK: u32 = 2;
const RET_SUBMIT: u32 = 3;
const RET_UNLINK: u32 = 4;

pub const DIRECTION_OUT: u32 = 0;
pub const DIRECTION_IN: u32 = 1;

pub const SPEED_FULL: u32 = 2;

/// `number_of_packets` of a URB that is not isochronous.
pub const NO_ISO_PACKETS: u32 = 0xffff_ffff;

pub const STATUS_OK: i32 = 0;
pub const STATUS_PIPE: i32 = -32; // EPIPE: stall
pub const STATUS_CONN_RESET: i32 = -104; // ECONNRESET: unlinked

/// The largest transfer we accept; audio and control URBs are a few KB.
const MAX_TRANSFER: u32 = 1024 * 1024;
const MAX_ISO_PACKETS: u32 = 1024;

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Reads the 8-byte operation header and returns its code.
pub fn read_op_header(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    let version = u16::from_be_bytes([b[0], b[1]]);
    if version != PROTOCOL_VERSION {
        return Err(invalid(format!("unsupported USB/IP version 0x{version:04x}")));
    }
    Ok(u16::from_be_bytes([b[2], b[3]]))
}

pub fn append_op_header(dst: &mut Vec<u8>, code: u16, status: u32) {
    dst.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    dst.extend_from_slice(&code.to_be_bytes());
    dst.extend_from_slice(&status.to_be_bytes());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BasicHeader {
    pub command: u32,
    pub sequence: u32,
    pub direction: u32,
    pub endpoint: u32,
}

pub fn read_basic_header(r: &mut impl Read) -> io::Result<BasicHeader> {
    let mut b = [0u8; 20];
    r.read_exact(&mut b)?;
    Ok(BasicHeader { command: be32(&b, 0), sequence: be32(&b, 4), direction: be32(&b, 12), endpoint: be32(&b, 16) })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IsoPacket {
    pub offset: u32,
    pub length: u32,
    pub actual_length: u32,
    pub status: i32,
}

/// A CMD_SUBMIT with its OUT payload and isochronous packet descriptors.
#[derive(Clone, Debug, Default)]
pub struct Submit {
    pub basic: BasicHeader,
    pub transfer_buffer_length: u32,
    pub number_of_packets: u32,
    pub setup: [u8; 8],
    pub out: Vec<u8>,
    pub packets: Vec<IsoPacket>,
}

impl Submit {
    pub fn is_isochronous(&self) -> bool {
        self.number_of_packets != NO_ISO_PACKETS && self.number_of_packets != 0
    }
}

/// Reads the rest of a CMD_SUBMIT after its basic header.
pub fn read_submit(r: &mut impl Read, basic: BasicHeader) -> io::Result<Submit> {
    let mut b = [0u8; 28];
    r.read_exact(&mut b)?;
    let mut submit = Submit {
        basic,
        transfer_buffer_length: be32(&b, 4),
        number_of_packets: be32(&b, 12),
        ..Default::default()
    };
    submit.setup.copy_from_slice(&b[20..28]);
    if submit.transfer_buffer_length > MAX_TRANSFER {
        return Err(invalid(format!("transfer length {} too large", submit.transfer_buffer_length)));
    }
    if basic.direction == DIRECTION_OUT && submit.transfer_buffer_length > 0 {
        submit.out.resize(submit.transfer_buffer_length as usize, 0);
        r.read_exact(&mut submit.out)?;
    }
    if submit.is_isochronous() {
        if submit.number_of_packets > MAX_ISO_PACKETS {
            return Err(invalid(format!("{} isochronous packets", submit.number_of_packets)));
        }
        for _ in 0..submit.number_of_packets {
            let mut d = [0u8; 16];
            r.read_exact(&mut d)?;
            submit.packets.push(IsoPacket {
                offset: be32(&d, 0),
                length: be32(&d, 4),
                actual_length: be32(&d, 8),
                status: be32(&d, 12) as i32,
            });
        }
    }
    Ok(submit)
}

/// Reads the body of a CMD_UNLINK and returns the sequence number to unlink.
pub fn read_unlink(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 28];
    r.read_exact(&mut b)?;
    Ok(be32(&b, 0))
}

/// Appends a USBIP_RET_SUBMIT. Replies carry zero devid, direction and
/// endpoint; the sequence number identifies the request.
pub fn append_ret_submit(
    dst: &mut Vec<u8>,
    submit: &Submit,
    status: i32,
    start_frame: u32,
    data: &[u8],
    packets: &[IsoPacket],
) {
    let (data, actual) = if status == STATUS_OK { (data, data.len() as u32) } else { (&[][..], 0) };
    let actual = if submit.basic.direction == DIRECTION_OUT && status == STATUS_OK {
        submit.out.len() as u32
    } else {
        actual
    };
    let number_of_packets = if submit.is_isochronous() { packets.len() as u32 } else { NO_ISO_PACKETS };
    let error_count = packets.iter().filter(|p| p.status != STATUS_OK).count() as u32;

    dst.extend_from_slice(&RET_SUBMIT.to_be_bytes());
    dst.extend_from_slice(&submit.basic.sequence.to_be_bytes());
    dst.extend_from_slice(&[0; 12]); // devid, direction, endpoint
    dst.extend_from_slice(&(status as u32).to_be_bytes());
    dst.extend_from_slice(&actual.to_be_bytes());
    dst.extend_from_slice(&start_frame.to_be_bytes());
    dst.extend_from_slice(&number_of_packets.to_be_bytes());
    dst.extend_from_slice(&error_count.to_be_bytes());
    dst.extend_from_slice(&[0; 8]); // padding
    if submit.basic.direction == DIRECTION_IN {
        dst.extend_from_slice(data);
    }
    for p in packets {
        dst.extend_from_slice(&p.offset.to_be_bytes());
        dst.extend_from_slice(&p.length.to_be_bytes());
        dst.extend_from_slice(&p.actual_length.to_be_bytes());
        dst.extend_from_slice(&(p.status as u32).to_be_bytes());
    }
}

pub fn append_ret_unlink(dst: &mut Vec<u8>, sequence: u32, status: i32) {
    dst.extend_from_slice(&RET_UNLINK.to_be_bytes());
    dst.extend_from_slice(&sequence.to_be_bytes());
    dst.extend_from_slice(&[0; 12]); // devid, direction, endpoint
    dst.extend_from_slice(&(status as u32).to_be_bytes());
    dst.extend_from_slice(&[0; 24]); // padding
}

/// Copies `value` into a fixed-size, zero-padded field.
pub fn fixed_string(dst: &mut [u8], value: &str) {
    dst.fill(0);
    let n = value.len().min(dst.len());
    dst[..n].copy_from_slice(&value.as_bytes()[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submit_bytes(direction: u32, endpoint: u32, length: u32, packets: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [CMD_SUBMIT, 7, 0x0001_0001, direction, endpoint] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        for v in [0, length, 0, packets, 0] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend_from_slice(&[0x80, 6, 0, 1, 0, 0, 18, 0]);
        b
    }

    #[test]
    fn parses_control_submit() {
        let wire = submit_bytes(DIRECTION_IN, 0, 18, NO_ISO_PACKETS);
        let mut r = wire.as_slice();
        let basic = read_basic_header(&mut r).unwrap();
        assert_eq!((basic.command, basic.sequence, basic.endpoint), (CMD_SUBMIT, 7, 0));
        let submit = read_submit(&mut r, basic).unwrap();
        assert!(!submit.is_isochronous());
        assert_eq!(submit.setup[1], 6, "GET_DESCRIPTOR");
        assert!(r.is_empty());
    }

    #[test]
    fn parses_iso_in_submit_descriptors() {
        let mut wire = submit_bytes(DIRECTION_IN, 1, 576, 2);
        for (offset, length) in [(0u32, 288u32), (288, 288)] {
            for v in [offset, length, 0, 0] {
                wire.extend_from_slice(&v.to_be_bytes());
            }
        }
        let mut r = wire.as_slice();
        let basic = read_basic_header(&mut r).unwrap();
        let submit = read_submit(&mut r, basic).unwrap();
        assert!(submit.is_isochronous());
        assert_eq!(submit.packets.len(), 2);
        assert_eq!(submit.packets[1].offset, 288);
        assert!(submit.out.is_empty(), "IN URBs carry no payload");
    }

    #[test]
    fn ret_submit_iso_layout() {
        let submit = Submit {
            basic: BasicHeader { sequence: 9, direction: DIRECTION_IN, endpoint: 1, ..Default::default() },
            number_of_packets: 2,
            ..Default::default()
        };
        let packets = [IsoPacket { length: 288, actual_length: 288, ..Default::default() }; 2];
        let mut wire = Vec::new();
        append_ret_submit(&mut wire, &submit, STATUS_OK, 123, &[0; 576], &packets);
        assert_eq!(be32(&wire, 0), RET_SUBMIT);
        assert_eq!(be32(&wire, 4), 9);
        assert_eq!(be32(&wire, 24), 576, "actual_length");
        assert_eq!(be32(&wire, 28), 123, "start_frame");
        assert_eq!(be32(&wire, 32), 2, "number_of_packets");
        assert_eq!(wire.len(), 48 + 576 + 2 * 16);
    }

    #[test]
    fn ret_submit_error_drops_payload() {
        let submit = Submit {
            basic: BasicHeader { direction: DIRECTION_IN, ..Default::default() },
            number_of_packets: NO_ISO_PACKETS,
            ..Default::default()
        };
        let mut wire = Vec::new();
        append_ret_submit(&mut wire, &submit, STATUS_PIPE, 0, &[1, 2, 3], &[]);
        assert_eq!(wire.len(), 48);
        assert_eq!(be32(&wire, 20) as i32, STATUS_PIPE);
        assert_eq!(be32(&wire, 32), NO_ISO_PACKETS);
    }

    #[test]
    fn ret_unlink_layout() {
        let mut wire = Vec::new();
        append_ret_unlink(&mut wire, 5, STATUS_CONN_RESET);
        assert_eq!(wire.len(), 48);
        assert_eq!((be32(&wire, 0), be32(&wire, 4)), (RET_UNLINK, 5));
        assert_eq!(be32(&wire, 20) as i32, STATUS_CONN_RESET);
    }
}
