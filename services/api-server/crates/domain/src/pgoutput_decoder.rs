use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PgOutputDecodeError {
    #[error("unexpected end of pgoutput payload while reading {0}")]
    UnexpectedEof(&'static str),
    #[error("unterminated cstring in pgoutput payload")]
    UnterminatedCString,
    #[error("unknown pgoutput message type: {0}")]
    UnknownMessageType(char),
    #[error("unsupported pgoutput message type: {0}")]
    UnsupportedMessageType(char),
    #[error("insert received before relation metadata for oid={0}")]
    MissingRelation(u32),
    #[error("unexpected tuple type for insert: {0}")]
    UnexpectedTupleType(char),
    #[error("tuple column count {actual} does not match relation column count {expected}")]
    ColumnCountMismatch { actual: usize, expected: usize },
    #[error("unsupported tuple value kind: {0}")]
    UnsupportedTupleValueKind(char),
    #[error("pgoutput text value is not valid utf-8")]
    InvalidUtf8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgOutputColumn {
    pub name: String,
    pub type_oid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgOutputRelation {
    pub relation_id: u32,
    pub namespace: String,
    pub relation_name: String,
    pub columns: Vec<PgOutputColumn>,
}

impl PgOutputRelation {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.namespace, self.relation_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgOutputInsert {
    pub relation: PgOutputRelation,
    pub values: HashMap<String, Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgOutputMessage {
    Relation(PgOutputRelation),
    Insert(PgOutputInsert),
}

#[derive(Debug, Default)]
pub struct PgOutputDecoder {
    relations: HashMap<u32, PgOutputRelation>,
}

impl PgOutputDecoder {
    pub fn decode(&mut self, payload: &[u8]) -> Result<Option<PgOutputMessage>, PgOutputDecodeError> {
        if payload.is_empty() {
            return Ok(None);
        }

        let mut reader = BufferReader::new(payload);
        let message_type = reader.read_byte()? as char;

        match message_type {
            'B' => {
                reader.read_u64()?;
                reader.read_u64()?;
                reader.read_i32()?;
                Ok(None)
            }
            'C' => {
                reader.read_byte()?;
                reader.read_u64()?;
                reader.read_u64()?;
                reader.read_u64()?;
                Ok(None)
            }
            'O' => {
                reader.read_u64()?;
                reader.read_cstring()?;
                Ok(None)
            }
            'Y' => Ok(None),
            'R' => {
                let relation = decode_relation(&mut reader)?;
                self.relations.insert(relation.relation_id, relation.clone());
                Ok(Some(PgOutputMessage::Relation(relation)))
            }
            'I' => {
                let insert = decode_insert(&mut reader, &self.relations)?;
                Ok(Some(PgOutputMessage::Insert(insert)))
            }
            'U' | 'D' | 'T' => Err(PgOutputDecodeError::UnsupportedMessageType(message_type)),
            _ => Err(PgOutputDecodeError::UnknownMessageType(message_type)),
        }
    }
}

fn decode_relation(reader: &mut BufferReader<'_>) -> Result<PgOutputRelation, PgOutputDecodeError> {
    let relation_id = reader.read_u32()?;
    let namespace = reader.read_cstring()?;
    let relation_name = reader.read_cstring()?;
    reader.read_byte()?;
    let column_count = reader.read_u16()? as usize;
    let mut columns = Vec::with_capacity(column_count);
    for _ in 0..column_count {
        reader.read_byte()?;
        let column_name = reader.read_cstring()?;
        let type_oid = reader.read_u32()?;
        reader.read_i32()?;
        columns.push(PgOutputColumn {
            name: column_name,
            type_oid,
        });
    }
    Ok(PgOutputRelation {
        relation_id,
        namespace,
        relation_name,
        columns,
    })
}

fn decode_insert(
    reader: &mut BufferReader<'_>,
    relations: &HashMap<u32, PgOutputRelation>,
) -> Result<PgOutputInsert, PgOutputDecodeError> {
    let relation_id = reader.read_u32()?;
    let relation = relations
        .get(&relation_id)
        .cloned()
        .ok_or(PgOutputDecodeError::MissingRelation(relation_id))?;

    let tuple_type = reader.read_byte()? as char;
    if tuple_type != 'N' {
        return Err(PgOutputDecodeError::UnexpectedTupleType(tuple_type));
    }

    let column_count = reader.read_u16()? as usize;
    if column_count != relation.columns.len() {
        return Err(PgOutputDecodeError::ColumnCountMismatch {
            actual: column_count,
            expected: relation.columns.len(),
        });
    }

    let mut values = HashMap::with_capacity(column_count);
    for column in &relation.columns {
        let kind = reader.read_byte()? as char;
        match kind {
            'n' | 'u' => {
                values.insert(column.name.clone(), None);
            }
            't' => {
                let value_length = reader.read_i32()?;
                let value_bytes = reader.read_bytes(value_length as usize)?;
                let value = std::str::from_utf8(value_bytes)
                    .map_err(|_| PgOutputDecodeError::InvalidUtf8)?
                    .to_string();
                values.insert(column.name.clone(), Some(value));
            }
            _ => return Err(PgOutputDecodeError::UnsupportedTupleValueKind(kind)),
        }
    }

    Ok(PgOutputInsert { relation, values })
}

#[derive(Debug)]
struct BufferReader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> BufferReader<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn read_byte(&mut self) -> Result<u8, PgOutputDecodeError> {
        if self.remaining() < 1 {
            return Err(PgOutputDecodeError::UnexpectedEof("byte"));
        }
        let value = self.payload[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_u16(&mut self) -> Result<u16, PgOutputDecodeError> {
        self.read_array::<2>("u16").map(u16::from_be_bytes)
    }

    fn read_u32(&mut self) -> Result<u32, PgOutputDecodeError> {
        self.read_array::<4>("u32").map(u32::from_be_bytes)
    }

    fn read_i32(&mut self) -> Result<i32, PgOutputDecodeError> {
        self.read_array::<4>("i32").map(i32::from_be_bytes)
    }

    fn read_u64(&mut self) -> Result<u64, PgOutputDecodeError> {
        self.read_array::<8>("u64").map(u64::from_be_bytes)
    }

    fn read_array<const N: usize>(&mut self, label: &'static str) -> Result<[u8; N], PgOutputDecodeError> {
        if self.remaining() < N {
            return Err(PgOutputDecodeError::UnexpectedEof(label));
        }
        let end = self.offset + N;
        let mut bytes = [0_u8; N];
        bytes.copy_from_slice(&self.payload[self.offset..end]);
        self.offset = end;
        Ok(bytes)
    }

    fn read_bytes(&mut self, size: usize) -> Result<&'a [u8], PgOutputDecodeError> {
        if self.remaining() < size {
            return Err(PgOutputDecodeError::UnexpectedEof("bytes"));
        }
        let start = self.offset;
        let end = start + size;
        self.offset = end;
        Ok(&self.payload[start..end])
    }

    fn read_cstring(&mut self) -> Result<String, PgOutputDecodeError> {
        let tail = &self.payload[self.offset..];
        let Some(end) = tail.iter().position(|byte| *byte == 0) else {
            return Err(PgOutputDecodeError::UnterminatedCString);
        };
        let value = std::str::from_utf8(&tail[..end])
            .map_err(|_| PgOutputDecodeError::InvalidUtf8)?
            .to_string();
        self.offset += end + 1;
        Ok(value)
    }

    fn remaining(&self) -> usize {
        self.payload.len().saturating_sub(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::{PgOutputDecodeError, PgOutputDecoder, PgOutputMessage};

    fn cstring(value: &str) -> Vec<u8> {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    #[test]
    fn decodes_relation_and_insert_for_domain_event_outbox() {
        let relation_id = 42_u32;
        let columns = ["event_id", "aggregate_type", "payload"];

        let mut relation = vec![b'R'];
        relation.extend_from_slice(&relation_id.to_be_bytes());
        relation.extend_from_slice(&cstring("public"));
        relation.extend_from_slice(&cstring("domain_event_outbox"));
        relation.push(0);
        relation.extend_from_slice(&(columns.len() as u16).to_be_bytes());
        for column in &columns {
            relation.push(0);
            relation.extend_from_slice(&cstring(column));
            relation.extend_from_slice(&25_u32.to_be_bytes());
            relation.extend_from_slice(&(-1_i32).to_be_bytes());
        }

        let mut insert = vec![b'I'];
        insert.extend_from_slice(&relation_id.to_be_bytes());
        insert.push(b'N');
        insert.extend_from_slice(&(columns.len() as u16).to_be_bytes());
        for value in ["evt-1", "flight", "{\"ok\":true}"] {
            insert.push(b't');
            insert.extend_from_slice(&(value.len() as i32).to_be_bytes());
            insert.extend_from_slice(value.as_bytes());
        }

        let mut decoder = PgOutputDecoder::default();
        let relation_message = decoder.decode(&relation).expect("decode relation");
        let insert_message = decoder.decode(&insert).expect("decode insert");

        match relation_message.expect("relation message") {
            PgOutputMessage::Relation(relation) => {
                assert_eq!(relation.full_name(), "public.domain_event_outbox");
                assert_eq!(relation.columns.len(), 3);
            }
            other => panic!("unexpected relation message: {other:?}"),
        }

        match insert_message.expect("insert message") {
            PgOutputMessage::Insert(insert) => {
                assert_eq!(insert.relation.full_name(), "public.domain_event_outbox");
                assert_eq!(insert.values["event_id"].as_deref(), Some("evt-1"));
                assert_eq!(insert.values["aggregate_type"].as_deref(), Some("flight"));
                assert_eq!(insert.values["payload"].as_deref(), Some("{\"ok\":true}"));
            }
            other => panic!("unexpected insert message: {other:?}"),
        }
    }

    #[test]
    fn rejects_insert_before_relation() {
        let mut insert = vec![b'I'];
        insert.extend_from_slice(&7_u32.to_be_bytes());
        insert.push(b'N');
        insert.extend_from_slice(&0_u16.to_be_bytes());

        let mut decoder = PgOutputDecoder::default();
        let error = decoder.decode(&insert).expect_err("missing relation should fail");
        assert_eq!(error, PgOutputDecodeError::MissingRelation(7));
    }
}
