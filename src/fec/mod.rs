use std::fmt::Debug;
use crate::Packet;
use crate::Result;
use crate::Error;
use networkcoding::RepairSymbol;
use networkcoding::SourceSymbolMetadata;

use tart::TartEncoder;
use tart::TartDecoder;

use self::maelstrom::MaelstromDecoder;
use self::maelstrom::MaelstromEncoder;
use self::maelstrom::MaelstromRepairInfo;
use self::maelstrom::MaelstromSSID;

#[derive(Clone, Debug)]
/// FEC scheme-specific metadata.
pub enum FecMetadata {
    Source(FecSourceMetadata),

    Repair(FecRepairMetadata),
}

impl FecMetadata {
    pub fn source(&self) -> Option<&FecSourceMetadata> {
        if let Self::Source(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub fn repair(&self) -> Option<&FecRepairMetadata> {
        if let Self::Repair(r) = self {
            Some(r)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
/// FEC scheme-specific source metadata.
pub enum FecSourceMetadata {
    Tart(SourceSymbolMetadata),
    Maelstrom(MaelstromSSID),
}

#[derive(Clone, Debug)]
/// FEC scheme-specific source metadata.
pub enum FecRepairMetadata {
    Tart(RepairSymbol),
    Maelstrom(MaelstromRepairInfo)
}

impl Packet {
    pub fn add_fec_metadata(&mut self, metadata: FecMetadata) -> Result<()> {
        if self.fec.is_some() {
            return Err(Error::FecDoubleMetadata);
        }
        self.fec = Some(metadata);

        Ok(())
    }
}

/// FEC encoder algorithm.
pub enum FecEncoder {
    Tart(TartEncoder),

    Maelstrom(MaelstromEncoder),

    None,
}

impl Debug for FecEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Maelstrom(m) => m.fmt(f),
            Self::Tart(t) => t.fmt(f),
        }
    }
}

impl FecEncoder {
    /// Add FEC metadata for the packet and protect it.
    pub fn protect_symbol(&mut self, pkt: &mut Packet) -> Result<()> {
        match self {
            Self::Tart(tart) => tart.protect_symbol(pkt),
            Self::Maelstrom(mael) => mael.protect_symbol(pkt),
            Self::None => Ok(()),
        }
    }

    /// Whether the encoder should generate repair symbols.
    pub fn should_generate_rs(&mut self) -> bool {
        match self {
            Self::None => false,
            Self::Tart(tart) => tart.should_send_rs(),
            Self::Maelstrom(mael) => mael.should_generate_rs(),
        }
    }

    /// Generate (potentially several) repair symbols.
    pub fn generate_rs(&mut self) -> Result<Vec<Packet>> {
        match self {
            Self::None => Ok(Vec::new()),
            Self::Tart(tart) => tart.generate_rs(),
            Self::Maelstrom(mael) => mael.generate_rs(),
        }
    }

    /// Receive feedback from the decoder.
    pub fn recv_feedback(&mut self, nb_lost: u64, nb_elems: u64) {
        if let Self::Tart(tart) = self {
            tart.recv_feedback(nb_lost, nb_elems)
        }
    }
}

/// FEC decoder algorithm.
pub enum FecDecoder {
    Tart(TartDecoder),

    Maelstrom(MaelstromDecoder),

    None,
}

impl FecDecoder {
    /// Receive a source symbol. Returns recovered packets.
    pub fn recv_ss(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        match self {
            Self::Tart(tart) => tart.recv_ss(pkt),
            Self::Maelstrom(mael) => mael.recv_ss(pkt),
            Self::None => Ok(Vec::new()),
        }
    }

    /// Receive a repair symbol. Returns recovered packets.
    pub fn recv_rs(&mut self, pkt: &Packet) -> Result<Vec<Packet>> {
        match self {
            Self::Tart(tart) => tart.recv_rs(pkt),
            Self::Maelstrom(mael) => mael.recv_rs(pkt),
            Self::None => Ok(Vec::new()),
        }
    }
}

pub mod tart;
pub mod maelstrom;
