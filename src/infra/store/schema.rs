use crate::infra::circuit::circuit::CircuitSeq;
use byteorder::{ByteOrder, LittleEndian};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gate {
    pub target: u8,
    pub c1: u8,
    pub c2: u8,
}

impl fmt::Debug for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Gate({}, {}, {})", self.target, self.c1, self.c2)
    }
}

/// Matches Python struct.pack format:
/// u64 (template_id)
/// u8 (basis_id)
/// u8 (width)
/// u16 (gate_count)
/// [u8; 32] (canonical_hash)
/// [u8; 32] (family_hash)
/// u8 (origin)
/// u64 (origin_template_id)
/// u32 (unroll_ops)
/// u16 (gates_len)
/// [gates_len * 3] (encoded gates)
#[derive(Debug, Clone)]
pub struct TemplateRecord {
    pub template_id: u64,
    pub basis_id: u8,
    pub width: u8,
    pub gate_count: u16,
    pub canonical_hash: [u8; 32],
    pub family_hash: [u8; 32],
    pub origin: u8,
    pub origin_template_id: u64,
    pub unroll_ops: u32,
    pub gates: Vec<Gate>,
}

impl TemplateRecord {
    pub const HEADER_SIZE: usize = 8 + 1 + 1 + 2 + 32 + 32 + 1 + 8 + 4 + 2; // 91 bytes

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::HEADER_SIZE {
            return None;
        }

        let template_id = LittleEndian::read_u64(&bytes[0..8]);
        let basis_id = bytes[8];
        let width = bytes[9];
        let gate_count = LittleEndian::read_u16(&bytes[10..12]);

        let mut canonical_hash = [0u8; 32];
        canonical_hash.copy_from_slice(&bytes[12..44]);

        let mut family_hash = [0u8; 32];
        family_hash.copy_from_slice(&bytes[44..76]);

        let origin = bytes[76];
        let origin_template_id = LittleEndian::read_u64(&bytes[77..85]);
        let unroll_ops = LittleEndian::read_u32(&bytes[85..89]);
        let gates_bytes_len = LittleEndian::read_u16(&bytes[89..91]) as usize;

        if bytes.len() != Self::HEADER_SIZE + gates_bytes_len {
            return None;
        }

        let num_gates = gates_bytes_len / 3;
        let mut gates = Vec::with_capacity(num_gates);
        let gates_slice = &bytes[91..];

        for i in 0..num_gates {
            let offset = i * 3;
            gates.push(Gate {
                target: gates_slice[offset],
                c1: gates_slice[offset + 1],
                c2: gates_slice[offset + 2],
            });
        }

        Some(Self {
            template_id,
            basis_id,
            width,
            gate_count,
            canonical_hash,
            family_hash,
            origin,
            origin_template_id,
            unroll_ops,
            gates,
        })
    }

    pub fn to_circuit_seq(&self) -> CircuitSeq {
        let gates: Vec<[u8; 3]> = self.gates.iter().map(|g| [g.target, g.c1, g.c2]).collect();
        CircuitSeq { gates }
    }
}
