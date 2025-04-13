use crate::error::Error;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tracing::{debug, trace};

/// Maximum message size for binary protocol (10MB)
const MAX_MESSAGE_SIZE: usize = 10_485_760;

/// Binary protocol message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Request = 0,
    Response = 1,
    Attestation = 2,
    KeepAlive = 3,
}

impl TryFrom<u8> for MessageType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MessageType::Request),
            1 => Ok(MessageType::Response),
            2 => Ok(MessageType::Attestation),
            3 => Ok(MessageType::KeepAlive),
            _ => Err(Error::Protocol(format!("Invalid message type: {}", value))),
        }
    }
}

/// Binary protocol message header
///
/// Layout:
/// - Magic (4 bytes): "ARTE" in ASCII
/// - Version (1 byte): Protocol version
/// - Type (1 byte): Message type
/// - Flags (2 bytes): Message flags
/// - Length (4 bytes): Length of the payload
/// - Request ID (8 bytes): Unique request identifier
#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub version: u8,
    pub msg_type: MessageType,
    pub flags: u16,
    pub length: u32,
    pub request_id: u64,
}

impl MessageHeader {
    /// Magic bytes for message identification ("ARTE")
    const MAGIC: [u8; 4] = [0x41, 0x52, 0x54, 0x45];
    
    /// Current protocol version
    const CURRENT_VERSION: u8 = 1;
    
    /// Size of the header in bytes
    pub const SIZE: usize = 20;
    
    pub fn new(msg_type: MessageType, length: u32, request_id: u64) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            msg_type,
            flags: 0,
            length,
            request_id,
        }
    }
    
    pub fn encode(&self, buf: &mut BytesMut) {
        buf.put_slice(&Self::MAGIC);
        buf.put_u8(self.version);
        buf.put_u8(self.msg_type as u8);
        buf.put_u16(self.flags);
        buf.put_u32(self.length);
        buf.put_u64(self.request_id);
    }
    
    pub fn decode(buf: &mut BytesMut) -> Result<Self, Error> {
        if buf.remaining() < Self::SIZE {
            return Err(Error::Protocol("Incomplete message header".to_string()));
        }
        
        let magic = [buf[0], buf[1], buf[2], buf[3]];
        if magic != Self::MAGIC {
            return Err(Error::Protocol(format!("Invalid magic bytes: {:?}", magic)));
        }
        
        buf.advance(4); // Skip magic
        
        let version = buf.get_u8();
        if version != Self::CURRENT_VERSION {
            return Err(Error::Protocol(format!("Unsupported protocol version: {}", version)));
        }
        
        let msg_type = MessageType::try_from(buf.get_u8())?;
        let flags = buf.get_u16();
        let length = buf.get_u32();
        
        if length as usize > MAX_MESSAGE_SIZE {
            return Err(Error::Protocol(format!("Message too large: {} bytes", length)));
        }
        
        let request_id = buf.get_u64();
        
        Ok(Self {
            version,
            msg_type,
            flags,
            length,
            request_id,
        })
    }
}

/// Binary protocol message
#[derive(Debug)]
pub struct Message<T> {
    pub header: MessageHeader,
    pub payload: T,
}

impl<T: Serialize> Message<T> {
    pub fn new(msg_type: MessageType, payload: T, request_id: u64) -> Result<Self, Error> {
        // Serialize payload to determine length
        let payload_bytes = bincode::serialize(&payload)
            .map_err(|e| Error::Protocol(format!("Failed to serialize payload: {}", e)))?;
            
        let length = payload_bytes.len() as u32;
        
        Ok(Self {
            header: MessageHeader::new(msg_type, length, request_id),
            payload,
        })
    }
    
    pub fn encode(&self) -> Result<BytesMut, Error> {
        let payload_bytes = bincode::serialize(&self.payload)
            .map_err(|e| Error::Protocol(format!("Failed to serialize payload: {}", e)))?;
            
        let total_len = MessageHeader::SIZE + payload_bytes.len();
        let mut buf = BytesMut::with_capacity(total_len);
        
        self.header.encode(&mut buf);
        buf.put_slice(&payload_bytes);
        
        Ok(buf)
    }
}

impl<T: for<'de> Deserialize<'de>> Message<T> {
    pub fn decode(header: MessageHeader, payload_bytes: &[u8]) -> Result<Self, Error> {
        let payload = bincode::deserialize(payload_bytes)
            .map_err(|e| Error::Protocol(format!("Failed to deserialize payload: {}", e)))?;
            
        Ok(Self {
            header,
            payload,
        })
    }
}

/// Parameter format detection and handling
///
/// Supports both:
/// - Length-prefixed format (4-byte length + data)
/// - Direct data format (raw data without length prefix)
pub struct ParameterData {
    pub data: Vec<u8>,
    pub format_type: ParameterFormatType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterFormatType {
    LengthPrefixed,
    Direct,
}

impl ParameterData {
    pub fn parse(bytes: &[u8], expected_direct_size: Option<usize>) -> Self {
        // Must have at least 4 bytes to check for length prefix
        if bytes.len() >= 4 {
            // Read potential length as little-endian u32
            let len_bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
            let potential_len = u32::from_le_bytes(len_bytes) as usize;
            
            // Check if the potential length is reasonable (< 1MB and fits in the buffer)
            if potential_len > 0 && potential_len < 1_048_576 && potential_len + 4 <= bytes.len() {
                let data = bytes[4..4 + potential_len].to_vec();
                debug!("Detected length-prefixed parameter: {} bytes", potential_len);
                return Self {
                    data,
                    format_type: ParameterFormatType::LengthPrefixed,
                };
            }
        }
        
        // If length prefix detection fails or we expect a direct size, treat as direct format
        let size = expected_direct_size.unwrap_or(bytes.len());
        let data = bytes.get(..size).unwrap_or(bytes).to_vec();
        debug!("Using direct parameter format: {} bytes", data.len());
        
        Self {
            data,
            format_type: ParameterFormatType::Direct,
        }
    }
    
    /// Create parameter bytes in length-prefixed format
    pub fn to_length_prefixed(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len() + 4);
        result.extend_from_slice(&(data.len() as u32).to_le_bytes());
        result.extend_from_slice(data);
        result
    }
    
    /// Create parameter bytes in direct format
    pub fn to_direct(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_header_encode_decode() {
        let header = MessageHeader::new(MessageType::Request, 42, 12345);
        let mut buf = BytesMut::with_capacity(MessageHeader::SIZE);
        header.encode(&mut buf);
        
        let decoded = MessageHeader::decode(&mut buf).unwrap();
        assert_eq!(decoded.version, header.version);
        assert_eq!(decoded.msg_type, header.msg_type);
        assert_eq!(decoded.flags, header.flags);
        assert_eq!(decoded.length, header.length);
        assert_eq!(decoded.request_id, header.request_id);
    }
    
    #[test]
    fn test_parameter_data_length_prefixed() {
        let data = b"test data";
        let len_bytes = (data.len() as u32).to_le_bytes();
        let mut prefixed = Vec::new();
        prefixed.extend_from_slice(&len_bytes);
        prefixed.extend_from_slice(data);
        
        let param = ParameterData::parse(&prefixed, None);
        assert_eq!(param.format_type, ParameterFormatType::LengthPrefixed);
        assert_eq!(param.data, data);
    }
    
    #[test]
    fn test_parameter_data_direct() {
        let data = b"test data";
        let param = ParameterData::parse(data, None);
        assert_eq!(param.format_type, ParameterFormatType::Direct);
        assert_eq!(param.data, data);
    }
    
    #[test]
    fn test_parameter_data_expected_size() {
        let data = b"test data longer than expected";
        let expected_size = 4;
        let param = ParameterData::parse(data, Some(expected_size));
        assert_eq!(param.format_type, ParameterFormatType::Direct);
        assert_eq!(param.data, &data[..expected_size]);
    }
}
